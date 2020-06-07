use actix_form_data::{Field, Form, Value};
use actix_web::{
    guard,
    http::header::{CacheControl, CacheDirective},
    middleware::Logger,
    web, App, HttpResponse, HttpServer,
};
use futures::stream::{Stream, TryStreamExt};
use log::{error, info};
use std::path::PathBuf;

mod error;
mod upload_manager;

use self::{error::UploadError, upload_manager::UploadManager};

const ACCEPTED_MIMES: &[mime::Mime] = &[
    mime::IMAGE_BMP,
    mime::IMAGE_GIF,
    mime::IMAGE_JPEG,
    mime::IMAGE_PNG,
];

const MEGABYTES: usize = 1024 * 1024;
const HOURS: u32 = 60 * 60;

// Try writing to a file
async fn safe_save_file(path: PathBuf, bytes: bytes::Bytes) -> Result<(), UploadError> {
    if let Some(path) = path.parent() {
        // create the directory for the file
        actix_fs::create_dir_all(path.to_owned()).await?;
    }

    // Only write the file if it doesn't already exist
    if let Err(e) = actix_fs::metadata(path.clone()).await {
        if e.kind() != Some(std::io::ErrorKind::NotFound) {
            return Err(e.into());
        }
    } else {
        return Ok(());
    }

    // Open the file for writing
    let file = actix_fs::file::create(path.clone()).await?;

    // try writing
    if let Err(e) = actix_fs::file::write(file, bytes).await {
        error!("Error writing file, {}", e);
        // remove file if writing failed before completion
        actix_fs::remove_file(path).await?;
        return Err(e.into());
    }

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
                "delete_token": delete_token,
            }));
        }
    }

    Ok(HttpResponse::Created().json(serde_json::json!({ "msg": "ok", "files": files })))
}

async fn delete(
    manager: web::Data<UploadManager>,
    path_entries: web::Path<(String, String)>,
) -> Result<HttpResponse, UploadError> {
    let (alias, token) = path_entries.into_inner();

    manager.delete(token, alias).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Serve original files
async fn serve(
    manager: web::Data<UploadManager>,
    alias: web::Path<String>,
) -> Result<HttpResponse, UploadError> {
    let filename = manager.from_alias(alias.into_inner()).await?;
    let mut path = manager.image_dir();
    path.push(filename);

    let ext = path
        .extension()
        .ok_or(UploadError::MissingExtension)?
        .to_owned();
    let ext = from_ext(ext);

    let stream = actix_fs::read_to_stream(path).await?;

    Ok(srv_response(stream, ext))
}

/// Serve resized files
async fn serve_resized(
    manager: web::Data<UploadManager>,
    path_entries: web::Path<(u32, String)>,
) -> Result<HttpResponse, UploadError> {
    use image::GenericImageView;

    let mut path = manager.image_dir();

    let (size, alias) = path_entries.into_inner();
    let name = manager.from_alias(alias).await?;
    path.push(size.to_string());
    path.push(name.clone());

    let ext = path
        .extension()
        .ok_or(UploadError::MissingExtension)?
        .to_owned();
    let ext = from_ext(ext);

    // If the thumbnail doesn't exist, we need to create it
    if let Err(e) = actix_fs::metadata(path.clone()).await {
        if e.kind() != Some(std::io::ErrorKind::NotFound) {
            error!("Error looking up thumbnail, {}", e);
            return Err(e.into());
        }

        let mut original_path = manager.image_dir();
        original_path.push(name.clone());

        // Read the image file & produce a DynamicImage
        //
        // Drop bytes so we don't keep it around in memory longer than we need to
        let (img, format) = {
            let bytes = actix_fs::read(original_path.clone()).await?;
            let format = image::guess_format(&bytes)?;
            let img = web::block(move || image::load_from_memory(&bytes)).await?;

            (img, format)
        };

        // return original image if resize target is larger
        if !img.in_bounds(size, size) {
            drop(img);
            let stream = actix_fs::read_to_stream(original_path).await?;
            return Ok(srv_response(stream, ext));
        }

        // perform thumbnail operation in a blocking thread
        let img_bytes: bytes::Bytes = web::block(move || {
            let mut bytes = std::io::Cursor::new(vec![]);
            img.thumbnail(size, size).write_to(&mut bytes, format)?;
            Ok(bytes::Bytes::from(bytes.into_inner())) as Result<_, image::error::ImageError>
        })
        .await?;

        let path2 = path.clone();
        let img_bytes2 = img_bytes.clone();

        // Save the file in another task, we want to return the thumbnail now
        actix_rt::spawn(async move {
            if let Err(e) = safe_save_file(path2, img_bytes2).await {
                error!("Error saving file, {}", e);
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

#[actix_rt::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let manager = UploadManager::new("data/".to_string().into()).await?;

    // Create a new Multipart Form validator
    //
    // This form is expecting a single array field, 'images' with at most 10 files in it
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
                        web::resource("/delete/{delete_token}/{filename}")
                            .route(web::delete().to(delete)),
                    )
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
