use actix_web::{http::StatusCode, HttpResponse, ResponseError};

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
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
            UploadError::NoFiles | UploadError::ContentType(_) | UploadError::Upload(_) => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({ "msg": self.to_string() }))
    }
}
