use crate::{error::UploadError, safe_save_file, to_ext, ACCEPTED_MIMES};
use actix_web::web;
use futures::stream::{Stream, StreamExt};
use sha2::Digest;
use std::{path::PathBuf, pin::Pin, sync::Arc};

#[derive(Clone)]
pub struct UploadManager {
    inner: Arc<UploadManagerInner>,
}

struct UploadManagerInner {
    hasher: sha2::Sha256,
    image_dir: PathBuf,
    db: sled::Db,
    alias_tree: sled::Tree,
}

type UploadStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, actix_form_data::Error>>>>;

enum Dup {
    Exists,
    New,
}

impl Dup {
    fn exists(&self) -> bool {
        match self {
            Dup::Exists => true,
            _ => false,
        }
    }
}

impl UploadManager {
    /// Get the image directory
    pub(crate) fn image_dir(&self) -> PathBuf {
        self.inner.image_dir.clone()
    }

    /// Create a new UploadManager
    pub(crate) async fn new(mut root_dir: PathBuf) -> Result<Self, UploadError> {
        let mut sled_dir = root_dir.clone();
        sled_dir.push("db");
        // sled automatically creates it's own directories
        //
        // This is technically a blocking operation but it's fine because it happens before we
        // start handling requests
        let db = sled::open(sled_dir)?;

        root_dir.push("files");

        // Ensure file dir exists
        actix_fs::create_dir_all(root_dir.clone()).await?;

        Ok(UploadManager {
            inner: Arc::new(UploadManagerInner {
                hasher: sha2::Sha256::new(),
                image_dir: root_dir,
                alias_tree: db.open_tree("alias")?,
                db,
            }),
        })
    }

    /// Upload the file, discarding bytes if it's already present, or saving if it's new
    pub(crate) async fn upload(
        &self,
        _filename: String,
        content_type: mime::Mime,
        mut stream: UploadStream,
    ) -> Result<Option<PathBuf>, UploadError> {
        if ACCEPTED_MIMES.iter().all(|valid| *valid != content_type) {
            return Err(UploadError::ContentType(content_type));
        }

        // -- READ IN BYTES FROM CLIENT --
        let mut bytes = bytes::BytesMut::new();

        while let Some(res) = stream.next().await {
            bytes.extend(res?);
        }

        let bytes = bytes.freeze();

        // -- DUPLICATE CHECKS --

        // Cloning bytes is fine because it's actually a pointer
        let hash = self.hash(bytes.clone()).await?;

        let alias = self.add_alias(&hash, content_type.clone()).await?;
        let (dup, name) = self.check_duplicate(hash, content_type).await?;

        // bail early with alias to existing file if this is a duplicate
        if dup.exists() {
            let mut path = PathBuf::new();
            path.push(alias);
            return Ok(Some(path));
        }

        // TODO: validate image before saving

        // -- WRITE NEW FILE --
        let mut real_path = self.image_dir();
        real_path.push(name);

        safe_save_file(real_path, bytes).await?;

        // Return alias to file
        let mut path = PathBuf::new();
        path.push(alias);
        Ok(Some(path))
    }

    pub(crate) async fn from_alias(&self, alias: String) -> Result<String, UploadError> {
        let tree = self.inner.alias_tree.clone();
        let hash = web::block(move || tree.get(alias.as_bytes()))
            .await?
            .ok_or(UploadError::MissingAlias)?;

        let db = self.inner.db.clone();
        let filename = web::block(move || db.get(hash))
            .await?
            .ok_or(UploadError::MissingFile)?;

        let filename = String::from_utf8(filename.to_vec())?;

        Ok(filename)
    }

    // produce a sh256sum of the uploaded file
    async fn hash(&self, bytes: bytes::Bytes) -> Result<Vec<u8>, UploadError> {
        let mut hasher = self.inner.hasher.clone();
        let hash = web::block(move || {
            hasher.input(&bytes);
            Ok(hasher.result().to_vec()) as Result<_, UploadError>
        })
        .await?;

        Ok(hash)
    }

    // check for an already-uploaded image with this hash, returning the path to the target file
    async fn check_duplicate(
        &self,
        hash: Vec<u8>,
        content_type: mime::Mime,
    ) -> Result<(Dup, String), UploadError> {
        let db = self.inner.db.clone();

        let filename = self.next_file(content_type).await?;
        let filename2 = filename.clone();
        let res = web::block(move || {
            db.compare_and_swap(hash, None as Option<sled::IVec>, Some(filename2.as_bytes()))
        })
        .await?;

        if let Err(sled::CompareAndSwapError {
            current: Some(ivec),
            ..
        }) = res
        {
            let name = String::from_utf8(ivec.to_vec())?;
            return Ok((Dup::Exists, name));
        }

        Ok((Dup::New, filename))
    }

    // generate a short filename that isn't already in-use
    async fn next_file(&self, content_type: mime::Mime) -> Result<String, UploadError> {
        let image_dir = self.image_dir();
        use rand::distributions::{Alphanumeric, Distribution};
        let mut limit: usize = 10;
        let rng = rand::thread_rng();
        loop {
            let mut path = image_dir.clone();
            let s: String = Alphanumeric.sample_iter(rng).take(limit).collect();

            let filename = file_name(s, content_type.clone());

            path.push(filename.clone());

            if let Err(e) = actix_fs::metadata(path).await {
                if e.kind() == Some(std::io::ErrorKind::NotFound) {
                    return Ok(filename);
                }
                return Err(e.into());
            }

            limit += 1;
        }
    }

    // Add an alias to an existing file
    //
    // This will help if multiple 'users' upload the same file, and one of them wants to delete it
    async fn add_alias(
        &self,
        hash: &[u8],
        content_type: mime::Mime,
    ) -> Result<String, UploadError> {
        let alias = self.next_alias(hash, content_type).await?;

        loop {
            let db = self.inner.db.clone();
            let id = web::block(move || db.generate_id()).await?.to_string();

            let mut key = hash.to_vec();
            key.extend(id.as_bytes());

            let db = self.inner.db.clone();
            let alias2 = alias.clone();
            let res = web::block(move || {
                db.compare_and_swap(key, None as Option<sled::IVec>, Some(alias2.as_bytes()))
            })
            .await?;

            if res.is_ok() {
                break;
            }
        }

        Ok(alias)
    }

    // Generate an alias to the file
    async fn next_alias(
        &self,
        hash: &[u8],
        content_type: mime::Mime,
    ) -> Result<String, UploadError> {
        use rand::distributions::{Alphanumeric, Distribution};
        let mut limit: usize = 10;
        let rng = rand::thread_rng();
        let hvec = hash.to_vec();
        loop {
            let s: String = Alphanumeric.sample_iter(rng).take(limit).collect();
            let filename = file_name(s, content_type.clone());

            let tree = self.inner.alias_tree.clone();
            let vec = hvec.clone();
            let filename2 = filename.clone();
            let res = web::block(move || {
                tree.compare_and_swap(filename2.as_bytes(), None as Option<sled::IVec>, Some(vec))
            })
            .await?;

            if res.is_ok() {
                return Ok(filename);
            }

            limit += 1;
        }
    }
}

fn file_name(name: String, content_type: mime::Mime) -> String {
    format!("{}{}", name, to_ext(content_type))
}
