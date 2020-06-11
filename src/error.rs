use crate::validate::GifError;
use actix_web::{http::StatusCode, HttpResponse, ResponseError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum UploadError {
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

    #[error("Error interacting with filesystem, {0}")]
    Io(#[from] std::io::Error),

    #[error("Panic in blocking operation")]
    Canceled,

    #[error("No files present in upload")]
    NoFiles,

    #[error("Uploaded image could not be served, extension is missing")]
    MissingExtension,

    #[error("Requested a file that doesn't exist")]
    MissingAlias,

    #[error("Alias directed to missing file")]
    MissingFile,

    #[error("Provided token did not match expected token")]
    InvalidToken,

    #[error("Uploaded content could not be validated as an image")]
    InvalidImage(image::error::ImageError),

    #[error("Unsupported image format")]
    UnsupportedFormat,

    #[error("Unable to download image, bad response {0}")]
    Download(actix_web::http::StatusCode),

    #[error("Unable to download image, {0}")]
    Payload(#[from] actix_web::client::PayloadError),

    #[error("Unable to send request, {0}")]
    SendRequest(String),

    #[error("No filename provided in request")]
    MissingFilename,

    #[error("Error converting Path to String")]
    Path,

    #[error("Tried to save an image with an already-taken name")]
    DuplicateAlias,

    #[error("Error validating Gif file, {0}")]
    Gif(#[from] GifError),
}

impl From<actix_web::client::SendRequestError> for UploadError {
    fn from(e: actix_web::client::SendRequestError) -> Self {
        UploadError::SendRequest(e.to_string())
    }
}

impl From<sled::transaction::TransactionError<UploadError>> for UploadError {
    fn from(e: sled::transaction::TransactionError<UploadError>) -> Self {
        match e {
            sled::transaction::TransactionError::Abort(t) => t,
            sled::transaction::TransactionError::Storage(e) => e.into(),
        }
    }
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
            UploadError::Gif(_)
            | UploadError::DuplicateAlias
            | UploadError::NoFiles
            | UploadError::Upload(_) => StatusCode::BAD_REQUEST,
            UploadError::MissingAlias | UploadError::MissingFilename => StatusCode::NOT_FOUND,
            UploadError::InvalidToken => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({ "msg": self.to_string() }))
    }
}
