use actix_form_data::{Field, Form, Value};
use actix_web::{
    guard,
    http::{
        header::{CacheControl, CacheDirective},
        StatusCode,
    },
    middleware::Logger,
    web, App, HttpResponse, HttpServer, ResponseError,
};
use futures::stream::{Stream, StreamExt, TryStreamExt};
use log::{error, info};
use sha2::Digest;
use std::{path::PathBuf, pin::Pin, sync::Arc};

const ACCEPTED_MIMES: &[mime::Mime] = &[
    mime::IMAGE_BMP,
    mime::IMAGE_GIF,
    mime::IMAGE_JPEG,
    mime::IMAGE_PNG,
];

const MEGABYTES: usize = 1024 * 1024;
const HOURS: u32 = 60 * 60;

#[derive(Clone)]
struct UploadManager {
    inner: Arc<UploadManagerInner>,
}

struct UploadManagerInner {
    hasher: sha2::Sha256,
    base_dir: PathBuf,
    db: sled::Db,
}

#[derive(Debug, thiserror::Error)]
enum UploadError {
    #[error("Invalid content type provided, {0}")]
    ContentType(mime::Mime),

    #[error("Couln't upload file, {0}")]
    Upload(String),

    #[error("Couldn't save file, {0}")]
    Save(#[from] actix_fs::Error),

    #[error("Error in DB, {0}")]
    Db(#[from] sled::Error),

    #[error("Error parsing string, {0}")]
    ParseString(#[from] std::string::FromUtf8Error),

    #[error("Error processing image, {0}")]
    Image(#[from] image::error::ImageError),

    #[error("Panic in blocking operation")]
    Canceled,

    #[error("No files present in upload")]
    NoFiles,

    #[error("Uploaded image could not be served, extension is missing")]
    MissingExtension,
}

impl From<actix_form_data::Error> for UploadError {
    fn from(e: actix_form_data::Error) -> Self {
        UploadError::Upload(e.to_string())
    }
}

impl<T> From<actix_web::error::BlockingError<T>> for UploadError
where
    T: Into<UploadError> + std::fmt::Debug,
{
    fn from(e: actix_web::error::BlockingError<T>) -> Self {
        match e {
            actix_web::error::BlockingError::Error(e) => e.into(),
            _ => UploadError::Canceled,
        }
    }
}

impl ResponseError for UploadError {
    fn status_code(&self) -> StatusCode {
        match self {
            UploadError::ContentType(_) | UploadError::Upload(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({ "msg": self.to_string() }))
    }
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
    async fn new(mut root_dir: PathBuf) -> Result<Self, UploadError> {
        let mut sled_dir = root_dir.clone();
        sled_dir.push("db");
        let db = sled::open(sled_dir)?;

        root_dir.push("files");

        actix_fs::create_dir_all(root_dir.clone()).await?;

        Ok(UploadManager {
            inner: Arc::new(UploadManagerInner {
                hasher: sha2::Sha256::new(),
                base_dir: root_dir,
                db,
            }),
        })
    }

    async fn upload(
        &self,
        _filename: String,
        content_type: mime::Mime,
        mut stream: UploadStream,
    ) -> Result<Option<PathBuf>, UploadError> {
        if ACCEPTED_MIMES.iter().all(|valid| *valid != content_type) {
            return Err(UploadError::ContentType(content_type));
        }

        let mut bytes = bytes::BytesMut::new();

        while let Some(res) = stream.next().await {
            bytes.extend(res?);
        }

        let bytes = bytes.freeze();

        let hash = self.hash(bytes.clone()).await?;

        let (dup, path) = self.check_duplicate(hash, content_type).await?;

        if dup.exists() {
            return Ok(Some(path));
        }

        let file = actix_fs::file::create(path.clone()).await?;

        if let Err(e) = actix_fs::file::write(file, bytes).await {
            error!("Error saving file, {}", e);
            actix_fs::remove_file(path).await?;
            return Err(e.into());
        }

        Ok(Some(path))
    }

    async fn hash(&self, bytes: bytes::Bytes) -> Result<Vec<u8>, UploadError> {
        let mut hasher = self.inner.hasher.clone();
        let hash = web::block(move || {
            hasher.input(&bytes);
            Ok(hasher.result().to_vec()) as Result<_, UploadError>
        })
        .await?;

        Ok(hash)
    }

    async fn check_duplicate(
        &self,
        hash: Vec<u8>,
        content_type: mime::Mime,
    ) -> Result<(Dup, PathBuf), UploadError> {
        let mut path = self.inner.base_dir.clone();
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
            path.push(name);

            return Ok((Dup::Exists, path));
        }

        path.push(filename);

        Ok((Dup::New, path))
    }

    async fn next_file(&self, content_type: mime::Mime) -> Result<String, UploadError> {
        let base_dir = self.inner.base_dir.clone();
        use rand::distributions::{Alphanumeric, Distribution};
        let mut limit: usize = 10;
        let rng = rand::thread_rng();
        loop {
            let mut path = base_dir.clone();
            let s: String = Alphanumeric.sample_iter(rng).take(limit).collect();

            let filename = format!("{}{}", s, to_ext(content_type.clone()));

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
}

fn to_ext(mime: mime::Mime) -> &'static str {
    if mime == mime::IMAGE_PNG {
        ".png"
    } else if mime == mime::IMAGE_JPEG {
        ".jpg"
    } else if mime == mime::IMAGE_GIF {
        ".gif"
    } else {
        ".bmp"
    }
}

fn from_ext(ext: std::ffi::OsString) -> mime::Mime {
    match ext.to_str() {
        Some("png") => mime::IMAGE_PNG,
        Some("jpg") => mime::IMAGE_JPEG,
        Some("gif") => mime::IMAGE_GIF,
        _ => mime::IMAGE_BMP,
    }
}

async fn upload(value: Value) -> Result<HttpResponse, UploadError> {
    let images = value
        .map()
        .and_then(|mut m| m.remove("images"))
        .and_then(|images| images.array())
        .ok_or(UploadError::NoFiles)?;

    let mut files = Vec::new();
    for image in images.into_iter().filter_map(|i| i.file()) {
        if let Some(saved_as) = image
            .saved_as
            .as_ref()
            .and_then(|s| s.file_name())
            .and_then(|s| s.to_str())
        {
            info!("Uploaded {} as {:?}", image.filename, saved_as);
            files.push(serde_json::json!({ "file": saved_as }));
        }
    }

    Ok(HttpResponse::Created().json(serde_json::json!({ "msg": "ok", "files": files })))
}

async fn serve(
    manager: web::Data<UploadManager>,
    filename: web::Path<String>,
) -> Result<HttpResponse, UploadError> {
    let mut path = manager.inner.base_dir.clone();
    path.push(filename.into_inner());
    let ext = path
        .extension()
        .ok_or(UploadError::MissingExtension)?
        .to_owned();
    let ext = from_ext(ext);

    let stream = actix_fs::read_to_stream(path).await?;

    Ok(srv_response(stream, ext))
}

async fn serve_resized(
    manager: web::Data<UploadManager>,
    filename: web::Path<(u32, String)>,
) -> Result<HttpResponse, UploadError> {
    use image::GenericImageView;

    let mut path = manager.inner.base_dir.clone();

    let (size, name) = filename.into_inner();
    path.push(size.to_string());
    path.push(name.clone());
    let ext = path
        .extension()
        .ok_or(UploadError::MissingExtension)?
        .to_owned();
    let ext = from_ext(ext);

    if let Err(e) = actix_fs::metadata(path.clone()).await {
        if e.kind() != Some(std::io::ErrorKind::NotFound) {
            error!("Error looking up thumbnail, {}", e);
            return Err(e.into());
        }

        let mut original_path = manager.inner.base_dir.clone();
        original_path.push(name.clone());

        let (img, format) = {
            let bytes = actix_fs::read(original_path.clone()).await?;
            let format = image::guess_format(&bytes)?;
            let img = image::load_from_memory(&bytes)?;

            (img, format)
        };

        if !img.in_bounds(size, size) {
            // return original image if resize target is larger
            drop(img);
            let stream = actix_fs::read_to_stream(original_path).await?;
            return Ok(srv_response(stream, ext));
        }

        let img_bytes: bytes::Bytes = web::block(move || {
            let mut bytes = std::io::Cursor::new(vec![]);
            img.thumbnail(size, size).write_to(&mut bytes, format)?;
            Ok(bytes::Bytes::from(bytes.into_inner())) as Result<_, image::error::ImageError>
        })
        .await?;

        let path2 = path.clone();
        let img_bytes2 = img_bytes.clone();

        actix_rt::spawn(async move {
            if let Some(path) = path2.parent() {
                if let Err(e) = actix_fs::create_dir_all(path.to_owned()).await {
                    error!("Couldn't create directory for thumbnail, {}", e);
                }
            }

            if let Err(e) = actix_fs::metadata(path2.clone()).await {
                if e.kind() == Some(std::io::ErrorKind::NotFound) {
                    if let Err(e) = actix_fs::write(path2, img_bytes2).await {
                        error!("Error saving image, {}", e);
                    }
                } else {
                    error!("Error checking image, {}", e);
                }
            }
        });

        return Ok(srv_response(
            Box::pin(futures::stream::once(async {
                Ok(img_bytes) as Result<_, UploadError>
            })),
            ext,
        ));
    }

    let stream = actix_fs::read_to_stream(path).await?;

    Ok(srv_response(stream, ext))
}

fn srv_response<S, E>(stream: S, ext: mime::Mime) -> HttpResponse
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Unpin + 'static,
    E: Into<UploadError>,
{
    HttpResponse::Ok()
        .set(CacheControl(vec![
            CacheDirective::Public,
            CacheDirective::MaxAge(24 * HOURS),
            CacheDirective::Extension("immutable".to_owned(), None),
        ]))
        .content_type(ext.to_string())
        .streaming(stream.err_into())
}

#[actix_rt::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let manager = UploadManager::new("data/".to_string().into()).await?;

    let manager2 = manager.clone();
    let form = Form::new()
        .max_files(10)
        .max_file_size(40 * MEGABYTES)
        .field(
            "images",
            Field::array(Field::file(move |filename, content_type, stream| {
                let manager = manager2.clone();

                async move { manager.upload(filename, content_type, stream).await }
            })),
        );

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .data(manager.clone())
            .service(
                web::scope("/image")
                    .service(
                        web::resource("")
                            .guard(guard::Post())
                            .wrap(form.clone())
                            .route(web::post().to(upload)),
                    )
                    .service(web::resource("/{filename}").route(web::get().to(serve)))
                    .service(
                        web::resource("/{size}/{filename}").route(web::get().to(serve_resized)),
                    ),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    Ok(())
}
