use actix_form_data::{Field, Form, Value};
use actix_web::{
    client::Client,
    guard,
    http::header::{CacheControl, CacheDirective},
    middleware::{Compress, Logger},
    web, App, HttpResponse, HttpServer,
};
use futures::stream::{Stream, TryStreamExt};
use once_cell::sync::Lazy;
use std::{collections::HashSet, path::PathBuf};
use structopt::StructOpt;
use tracing::{debug, error, info, instrument, Span};
use tracing_subscriber::EnvFilter;

mod config;
mod error;
mod middleware;
mod processor;
mod upload_manager;
mod validate;

use self::{
    config::Config, error::UploadError, middleware::Tracing, upload_manager::UploadManager,
};

const MEGABYTES: usize = 1024 * 1024;
const HOURS: u32 = 60 * 60;

static CONFIG: Lazy<Config> = Lazy::new(|| Config::from_args());

// Try writing to a file
#[instrument(skip(bytes))]
async fn safe_save_file(path: PathBuf, bytes: bytes::Bytes) -> Result<(), UploadError> {
    if let Some(path) = path.parent() {
        // create the directory for the file
        debug!("Creating directory {:?}", path);
        actix_fs::create_dir_all(path.to_owned()).await?;
    }

    // Only write the file if it doesn't already exist
    debug!("Checking if {:?} already exists", path);
    if let Err(e) = actix_fs::metadata(path.clone()).await {
        if e.kind() != Some(std::io::ErrorKind::NotFound) {
            return Err(e.into());
        }
    } else {
        return Ok(());
    }

    // Open the file for writing
    debug!("Creating {:?}", path);
    let file = actix_fs::file::create(path.clone()).await?;

    // try writing
    debug!("Writing to {:?}", path);
    if let Err(e) = actix_fs::file::write(file, bytes).await {
        error!("Error writing {:?}, {}", path, e);
        // remove file if writing failed before completion
        actix_fs::remove_file(path).await?;
        return Err(e.into());
    }
    debug!("{:?} written", path);

    Ok(())
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

/// Handle responding to succesful uploads
#[instrument(skip(manager))]
async fn upload(
    value: Value,
    manager: web::Data<UploadManager>,
) -> Result<HttpResponse, UploadError> {
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
            let delete_token = manager.delete_token(saved_as.to_owned()).await?;
            files.push(serde_json::json!({
                "file": saved_as,
                "delete_token": delete_token
            }));
        }
    }

    Ok(HttpResponse::Created().json(serde_json::json!({
        "msg": "ok",
        "files": files
    })))
}

/// download an image from a URL
#[instrument(skip(client, manager))]
async fn download(
    client: web::Data<Client>,
    manager: web::Data<UploadManager>,
    query: web::Query<UrlQuery>,
) -> Result<HttpResponse, UploadError> {
    let mut res = client.get(&query.url).send().await?;

    if !res.status().is_success() {
        return Err(UploadError::Download(res.status()));
    }

    let fut = res.body().limit(CONFIG.max_file_size() * MEGABYTES);

    let stream = Box::pin(futures::stream::once(fut));

    let alias = manager.upload(stream).await?;
    let delete_token = manager.delete_token(alias.clone()).await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "msg": "ok",
        "files": [{
            "file": alias,
            "delete_token": delete_token
        }]
    })))
}

#[instrument(skip(manager))]
async fn delete(
    manager: web::Data<UploadManager>,
    path_entries: web::Path<(String, String)>,
) -> Result<HttpResponse, UploadError> {
    let (alias, token) = path_entries.into_inner();

    manager.delete(token, alias).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Serve files
#[instrument(skip(manager))]
async fn serve(
    segments: web::Path<String>,
    manager: web::Data<UploadManager>,
    whitelist: web::Data<Option<HashSet<String>>>,
) -> Result<HttpResponse, UploadError> {
    let mut segments: Vec<String> = segments
        .into_inner()
        .split('/')
        .map(|s| s.to_string())
        .collect();
    let alias = segments.pop().ok_or(UploadError::MissingFilename)?;

    debug!("Building chain");
    let chain = self::processor::build_chain(&segments, whitelist.as_ref().as_ref());
    debug!("Chain built");

    let name = manager.from_alias(alias).await?;
    let base = manager.image_dir();
    let path = self::processor::build_path(base, &chain, name.clone());

    let ext = path
        .extension()
        .ok_or(UploadError::MissingExtension)?
        .to_owned();
    let ext = from_ext(ext);

    // If the thumbnail doesn't exist, we need to create it
    if let Err(e) = actix_fs::metadata(path.clone()).await {
        if e.kind() != Some(std::io::ErrorKind::NotFound) {
            error!("Error looking up processed image, {}", e);
            return Err(e.into());
        }

        let mut original_path = manager.image_dir();
        original_path.push(name.clone());

        // Read the image file & produce a DynamicImage
        //
        // Drop bytes so we don't keep it around in memory longer than we need to
        debug!("Reading image");
        let (img, format) = {
            let bytes = actix_fs::read(original_path.clone()).await?;
            let bytes2 = bytes.clone();
            let format = web::block(move || image::guess_format(&bytes2)).await?;
            let img = web::block(move || image::load_from_memory(&bytes)).await?;

            (img, format)
        };

        debug!("Processing image");
        let img = self::processor::process_image(chain, img).await?;

        // perform thumbnail operation in a blocking thread
        debug!("Exporting image");
        let img_bytes: bytes::Bytes = web::block(move || {
            let mut bytes = std::io::Cursor::new(vec![]);
            img.write_to(&mut bytes, format)?;
            Ok(bytes::Bytes::from(bytes.into_inner())) as Result<_, image::error::ImageError>
        })
        .await?;

        let path2 = path.clone();
        let img_bytes2 = img_bytes.clone();

        // Save the file in another task, we want to return the thumbnail now
        debug!("Spawning storage task");
        let span = Span::current();
        actix_rt::spawn(async move {
            let entered = span.enter();
            if let Err(e) = manager.store_variant(path2.clone()).await {
                error!("Error storing variant, {}", e);
                return;
            }

            if let Err(e) = safe_save_file(path2, img_bytes2).await {
                error!("Error saving file, {}", e);
            }
            drop(entered);
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

// A helper method to produce responses with proper cache headers
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

#[derive(Debug, serde::Deserialize)]
struct UrlQuery {
    url: String,
}

#[actix_rt::main]
async fn main() -> Result<(), anyhow::Error> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let manager = UploadManager::new(CONFIG.data_dir(), CONFIG.format()).await?;

    // Create a new Multipart Form validator
    //
    // This form is expecting a single array field, 'images' with at most 10 files in it
    let manager2 = manager.clone();
    let form = Form::new()
        .max_files(10)
        .max_file_size(CONFIG.max_file_size() * MEGABYTES)
        .transform_error(|e| UploadError::from(e).into())
        .field(
            "images",
            Field::array(Field::file(move |_, _, stream| {
                let manager = manager2.clone();

                async move {
                    manager.upload(stream).await.map(|alias| {
                        let mut path = PathBuf::new();
                        path.push(alias);
                        Some(path)
                    })
                }
            })),
        );

    // Create a new Multipart Form validator for internal imports
    //
    // This form is expecting a single array field, 'images' with at most 10 files in it
    let validate_imports = CONFIG.validate_imports();
    let manager2 = manager.clone();
    let import_form = Form::new()
        .max_files(10)
        .max_file_size(CONFIG.max_file_size() * MEGABYTES)
        .transform_error(|e| UploadError::from(e).into())
        .field(
            "images",
            Field::array(Field::file(move |filename, content_type, stream| {
                let manager = manager2.clone();

                async move {
                    manager
                        .import(filename, content_type, validate_imports, stream)
                        .await
                        .map(|alias| {
                            let mut path = PathBuf::new();
                            path.push(alias);
                            Some(path)
                        })
                }
            })),
        );

    HttpServer::new(move || {
        let client = Client::build()
            .header("User-Agent", "pict-rs v0.1.0-master")
            .finish();

        App::new()
            .wrap(Compress::default())
            .wrap(Logger::default())
            .wrap(Tracing)
            .data(manager.clone())
            .data(client)
            .data(CONFIG.filter_whitelist())
            .service(
                web::scope("/image")
                    .service(
                        web::resource("")
                            .guard(guard::Post())
                            .wrap(form.clone())
                            .route(web::post().to(upload)),
                    )
                    .service(web::resource("/download").route(web::get().to(download)))
                    .service(
                        web::resource("/delete/{delete_token}/{filename}")
                            .route(web::delete().to(delete))
                            .route(web::get().to(delete)),
                    )
                    .service(web::resource("/{tail:.*}").route(web::get().to(serve))),
            )
            .service(
                web::resource("/import")
                    .wrap(import_form.clone())
                    .route(web::post().to(upload)),
            )
    })
    .bind(CONFIG.bind_address())?
    .run()
    .await?;

    Ok(())
}
