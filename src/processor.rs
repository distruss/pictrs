use crate::error::UploadError;
use actix_web::web;
use image::{DynamicImage, GenericImageView};
use log::warn;
use std::path::PathBuf;

pub(crate) trait Processor {
    fn path(&self, path: PathBuf) -> PathBuf;
    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError>;
}

pub(crate) struct Identity;

impl Processor for Identity {
    fn path(&self, path: PathBuf) -> PathBuf {
        path
    }

    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError> {
        Ok(img)
    }
}

pub(crate) struct Thumbnail(u32);

impl Processor for Thumbnail {
    fn path(&self, mut path: PathBuf) -> PathBuf {
        path.push("thumbnail");
        path.push(self.0.to_string());
        path
    }

    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError> {
        if img.in_bounds(self.0, self.0) {
            Ok(img.thumbnail(self.0, self.0))
        } else {
            Ok(img)
        }
    }
}

pub(crate) struct Blur(f32);

impl Processor for Blur {
    fn path(&self, mut path: PathBuf) -> PathBuf {
        path.push("blur");
        path.push(self.0.to_string());
        path
    }

    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError> {
        Ok(img.blur(self.0))
    }
}

pub(crate) fn build_chain(args: &[String]) -> Vec<Box<dyn Processor + Send>> {
    args.into_iter().fold(Vec::new(), |mut acc, arg| {
        match arg.to_lowercase().as_str() {
            "identity" => acc.push(Box::new(Identity)),
            other if other.starts_with("blur") => {
                if let Ok(sigma) = other.trim_start_matches("blur").parse() {
                    acc.push(Box::new(Blur(sigma)));
                }
            }
            other => {
                if let Ok(size) = other.parse() {
                    acc.push(Box::new(Thumbnail(size)));
                } else {
                    warn!("Unknown processor {}", other);
                }
            }
        };
        acc
    })
}

pub(crate) fn build_path(
    base: PathBuf,
    args: &[Box<dyn Processor + Send>],
    filename: String,
) -> PathBuf {
    let mut path = args.iter().fold(base, |acc, processor| processor.path(acc));

    path.push(filename);
    path
}

pub(crate) async fn process_image(
    args: Vec<Box<dyn Processor + Send>>,
    mut img: DynamicImage,
) -> Result<DynamicImage, UploadError> {
    for processor in args.into_iter() {
        img = web::block(move || processor.process(img)).await?;
    }

    Ok(img)
}
