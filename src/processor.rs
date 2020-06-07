use crate::error::UploadError;
use actix_web::web;
use image::{DynamicImage, GenericImageView};
use log::debug;
use std::{collections::HashSet, path::PathBuf};

pub(crate) trait Processor {
    fn name() -> &'static str
    where
        Self: Sized;

    fn is_processor(s: &str) -> bool
    where
        Self: Sized;

    fn parse(s: &str) -> Option<Box<dyn Processor + Send>>
    where
        Self: Sized;

    fn path(&self, path: PathBuf) -> PathBuf;
    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError>;

    fn is_whitelisted(whitelist: Option<&HashSet<String>>) -> bool
    where
        Self: Sized,
    {
        whitelist
            .map(|wl| wl.contains(Self::name()))
            .unwrap_or(true)
    }
}

pub(crate) struct Identity;

impl Processor for Identity {
    fn name() -> &'static str
    where
        Self: Sized,
    {
        "identity"
    }

    fn is_processor(s: &str) -> bool
    where
        Self: Sized,
    {
        s == Self::name()
    }

    fn parse(_: &str) -> Option<Box<dyn Processor + Send>>
    where
        Self: Sized,
    {
        Some(Box::new(Identity))
    }

    fn path(&self, path: PathBuf) -> PathBuf {
        path
    }

    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError> {
        Ok(img)
    }
}

pub(crate) struct Thumbnail(u32);

impl Processor for Thumbnail {
    fn name() -> &'static str
    where
        Self: Sized,
    {
        "thumbnail"
    }

    fn is_processor(s: &str) -> bool
    where
        Self: Sized,
    {
        s.starts_with(Self::name())
    }

    fn parse(s: &str) -> Option<Box<dyn Processor + Send>>
    where
        Self: Sized,
    {
        let size = s.trim_start_matches(Self::name()).parse().ok()?;
        Some(Box::new(Thumbnail(size)))
    }

    fn path(&self, mut path: PathBuf) -> PathBuf {
        path.push(Self::name());
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
    fn name() -> &'static str
    where
        Self: Sized,
    {
        "blur"
    }

    fn is_processor(s: &str) -> bool {
        s.starts_with(Self::name())
    }

    fn parse(s: &str) -> Option<Box<dyn Processor + Send>> {
        let sigma = s.trim_start_matches(Self::name()).parse().ok()?;
        Some(Box::new(Blur(sigma)))
    }

    fn path(&self, mut path: PathBuf) -> PathBuf {
        path.push(Self::name());
        path.push(self.0.to_string());
        path
    }

    fn process(&self, img: DynamicImage) -> Result<DynamicImage, UploadError> {
        Ok(img.blur(self.0))
    }
}

macro_rules! parse {
    ($x:ident, $y:expr, $z:expr) => {{
        if $x::is_processor($y) && $x::is_whitelisted($z) {
            return $x::parse($y);
        }
    }};
}

pub(crate) fn build_chain(
    args: &[String],
    whitelist: Option<&HashSet<String>>,
) -> Vec<Box<dyn Processor + Send>> {
    args.into_iter()
        .filter_map(|arg| {
            parse!(Identity, arg.as_str(), whitelist);
            parse!(Thumbnail, arg.as_str(), whitelist);
            parse!(Blur, arg.as_str(), whitelist);

            debug!("Skipping {}, invalid or whitelisted", arg);

            None
        })
        .collect()
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
