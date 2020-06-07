use crate::{config::Format, error::UploadError, safe_save_file, to_ext, ACCEPTED_MIMES};
use actix_web::web;
use futures::stream::{Stream, StreamExt};
use log::{error, warn};
use sha2::Digest;
use std::{path::PathBuf, pin::Pin, sync::Arc};

#[derive(Clone)]
pub struct UploadManager {
    inner: Arc<UploadManagerInner>,
}

struct UploadManagerInner {
    format: Option<Format>,
    hasher: sha2::Sha256,
    image_dir: PathBuf,
    alias_tree: sled::Tree,
    db: sled::Db,
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
    pub(crate) async fn new(
        mut root_dir: PathBuf,
        format: Option<Format>,
    ) -> Result<Self, UploadError> {
        let mut sled_dir = root_dir.clone();
        sled_dir.push("db");
        // sled automatically creates it's own directories
        let db = web::block(move || sled::open(sled_dir)).await?;

        root_dir.push("files");

        // Ensure file dir exists
        actix_fs::create_dir_all(root_dir.clone()).await?;

        Ok(UploadManager {
            inner: Arc::new(UploadManagerInner {
                format,
                hasher: sha2::Sha256::new(),
                image_dir: root_dir,
                alias_tree: db.open_tree("alias")?,
                db,
            }),
        })
    }

    pub(crate) async fn delete(&self, alias: String, token: String) -> Result<(), UploadError> {
        use sled::Transactional;
        let db = self.inner.db.clone();
        let alias_tree = self.inner.alias_tree.clone();

        let alias2 = alias.clone();
        let hash = web::block(move || {
            [&*db, &alias_tree].transaction(|v| {
                let db = &v[0];
                let alias_tree = &v[1];

                // -- GET TOKEN --
                let existing_token = alias_tree
                    .remove(delete_key(&alias2).as_bytes())?
                    .ok_or(trans_err(UploadError::MissingAlias))?;

                // Bail if invalid token
                if existing_token != token {
                    warn!("Invalid delete token");
                    return Err(trans_err(UploadError::InvalidToken));
                }

                // -- GET ID FOR HASH TREE CLEANUP --
                let id = alias_tree
                    .remove(alias_id_key(&alias2).as_bytes())?
                    .ok_or(trans_err(UploadError::MissingAlias))?;
                let id = String::from_utf8(id.to_vec()).map_err(|e| trans_err(e.into()))?;

                // -- GET HASH FOR HASH TREE CLEANUP --
                let hash = alias_tree
                    .remove(alias2.as_bytes())?
                    .ok_or(trans_err(UploadError::MissingAlias))?;

                // -- REMOVE HASH TREE ELEMENT --
                db.remove(alias_key(&hash, &id))?;
                Ok(hash)
            })
        })
        .await?;

        // -- CHECK IF ANY OTHER ALIASES EXIST --
        let db = self.inner.db.clone();
        let (start, end) = alias_key_bounds(&hash);
        let any_aliases = web::block(move || {
            Ok(db.range(start..end).next().is_some()) as Result<bool, UploadError>
        })
        .await?;

        // Bail if there are existing aliases
        if any_aliases {
            return Ok(());
        }

        // -- DELETE HASH ENTRY --
        let db = self.inner.db.clone();
        let real_filename = web::block(move || {
            let real_filename = db.remove(&hash)?.ok_or(UploadError::MissingFile)?;

            Ok(real_filename) as Result<sled::IVec, UploadError>
        })
        .await?;

        let real_filename = String::from_utf8(real_filename.to_vec())?;

        let image_dir = self.image_dir();

        // -- DELETE FILES --
        actix_rt::spawn(async move {
            if let Err(e) =
                web::block(move || blocking_delete_all_by_filename(image_dir, &real_filename)).await
            {
                error!("Error removing files from fs, {}", e);
            }
        });

        Ok(())
    }

    /// Generate a delete token for an alias
    pub(crate) async fn delete_token(&self, alias: String) -> Result<String, UploadError> {
        use rand::distributions::{Alphanumeric, Distribution};
        let rng = rand::thread_rng();
        let s: String = Alphanumeric.sample_iter(rng).take(10).collect();
        let delete_token = s.clone();

        let alias_tree = self.inner.alias_tree.clone();
        let key = delete_key(&alias);
        let res = web::block(move || {
            alias_tree.compare_and_swap(
                key.as_bytes(),
                None as Option<sled::IVec>,
                Some(s.as_bytes()),
            )
        })
        .await?;

        if let Err(sled::CompareAndSwapError {
            current: Some(ivec),
            ..
        }) = res
        {
            let s = String::from_utf8(ivec.to_vec())?;

            return Ok(s);
        }

        Ok(delete_token)
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

        let (img, format) = {
            // -- READ IN BYTES FROM CLIENT --
            let mut bytes = bytes::BytesMut::new();

            while let Some(res) = stream.next().await {
                bytes.extend(res?);
            }

            let bytes = bytes.freeze();

            // -- VALIDATE IMAGE --
            let format = image::guess_format(&bytes).map_err(UploadError::InvalidImage)?;
            let img = image::load_from_memory(&bytes).map_err(UploadError::InvalidImage)?;

            (img, format)
        };

        let format = self
            .inner
            .format
            .as_ref()
            .map(|f| f.to_image_format())
            .unwrap_or(format);

        let content_type = self
            .inner
            .format
            .as_ref()
            .map(|f| f.to_mime())
            .unwrap_or(content_type);

        let bytes: bytes::Bytes = {
            let mut bytes = std::io::Cursor::new(vec![]);
            img.write_to(&mut bytes, format)?;
            bytes::Bytes::from(bytes.into_inner())
        };

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

            let key = alias_key(hash, &id);
            let db = self.inner.db.clone();
            let alias2 = alias.clone();
            let res = web::block(move || {
                db.compare_and_swap(key, None as Option<sled::IVec>, Some(alias2.as_bytes()))
            })
            .await?;

            if res.is_ok() {
                let alias_tree = self.inner.alias_tree.clone();
                let key = alias_id_key(&alias);
                web::block(move || alias_tree.insert(key.as_bytes(), id.as_bytes())).await?;

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

fn blocking_delete_all_by_filename(mut dir: PathBuf, filename: &str) -> Result<(), UploadError> {
    for res in std::fs::read_dir(dir.clone())? {
        let entry = res?;

        if entry.path().is_dir() {
            blocking_delete_all_by_filename(entry.path(), filename)?;
        }
    }

    dir.push(filename);

    if dir.is_file() {
        std::fs::remove_file(dir)?;
    }

    Ok(())
}

fn trans_err(e: UploadError) -> sled::transaction::ConflictableTransactionError<UploadError> {
    sled::transaction::ConflictableTransactionError::Abort(e)
}

fn file_name(name: String, content_type: mime::Mime) -> String {
    format!("{}{}", name, to_ext(content_type))
}

fn alias_key(hash: &[u8], id: &str) -> Vec<u8> {
    let mut key = hash.to_vec();
    // add a separator to the key between the hash and the ID
    key.extend(&[0]);
    key.extend(id.as_bytes());

    key
}

fn alias_key_bounds(hash: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut start = hash.to_vec();
    start.extend(&[0]);

    let mut end = hash.to_vec();
    end.extend(&[1]);

    (start, end)
}

fn alias_id_key(alias: &str) -> String {
    format!("{}/id", alias)
}

fn delete_key(alias: &str) -> String {
    format!("{}/delete", alias)
}
