use crate::{
    error::UploadError,
    validate::{ptos, Op},
};
use actix_web::web;
use bytes::Bytes;
use magick_rust::MagickWand;
use std::{collections::HashSet, path::PathBuf};
use tracing::{debug, instrument, Span};

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
    fn process(&self, wand: &mut MagickWand) -> Result<bool, UploadError>;

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
        debug!("Identity");
        Some(Box::new(Identity))
    }

    fn path(&self, path: PathBuf) -> PathBuf {
        path
    }

    fn process(&self, _: &mut MagickWand) -> Result<bool, UploadError> {
        Ok(false)
    }
}

pub(crate) struct Thumbnail(usize);

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

    fn process(&self, wand: &mut MagickWand) -> Result<bool, UploadError> {
        debug!("Thumbnail");
        let width = wand.get_image_width();
        let height = wand.get_image_height();

        if width > self.0 || height > self.0 {
            let width_ratio = width as f64 / self.0 as f64;
            let height_ratio = height as f64 / self.0 as f64;

            let (new_width, new_height) = if width_ratio < height_ratio {
                (width as f64 / height_ratio, self.0 as f64)
            } else {
                (self.0 as f64, height as f64 / width_ratio)
            };

            wand.op(|w| w.sample_image(new_width as usize, new_height as usize))?;
            Ok(true)
        } else if wand.op(|w| w.get_image_format())? == "GIF" {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub(crate) struct Blur(f64);

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

    fn process(&self, wand: &mut MagickWand) -> Result<bool, UploadError> {
        debug!("Blur");
        if self.0 > 0.0 {
            wand.op(|w| w.gaussian_blur_image(0.0, self.0))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

macro_rules! parse {
    ($x:ident, $y:expr, $z:expr) => {{
        if $x::is_processor($y) && $x::is_whitelisted($z) {
            return $x::parse($y);
        }
    }};
}

pub(crate) struct ProcessChain {
    inner: Vec<Box<dyn Processor + Send>>,
}

impl std::fmt::Debug for ProcessChain {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("ProcessChain")
            .field("steps", &self.inner.len())
            .finish()
    }
}

#[instrument]
pub(crate) fn build_chain(args: &[String], whitelist: Option<&HashSet<String>>) -> ProcessChain {
    let inner = args
        .into_iter()
        .filter_map(|arg| {
            parse!(Identity, arg.as_str(), whitelist);
            parse!(Thumbnail, arg.as_str(), whitelist);
            parse!(Blur, arg.as_str(), whitelist);

            debug!("Skipping {}, invalid or whitelisted", arg);

            None
        })
        .collect();

    ProcessChain { inner }
}

pub(crate) fn build_path(base: PathBuf, chain: &ProcessChain, filename: String) -> PathBuf {
    let mut path = chain
        .inner
        .iter()
        .fold(base, |acc, processor| processor.path(acc));

    path.push(filename);
    path
}

#[instrument]
pub(crate) async fn process_image(
    original_file: PathBuf,
    chain: ProcessChain,
) -> Result<Option<Bytes>, UploadError> {
    let original_path_str = ptos(&original_file)?;
    let span = Span::current();

    let opt = web::block(move || {
        let entered = span.enter();

        let mut wand = MagickWand::new();
        debug!("Reading image");
        wand.op(|w| w.read_image(&original_path_str))?;

        let format = wand.op(|w| w.get_image_format())?;

        debug!("Processing image");
        let mut changed = false;

        for processor in chain.inner.into_iter() {
            debug!("Step");
            changed |= processor.process(&mut wand)?;
            debug!("Step complete");
        }

        if changed {
            let vec = wand.op(|w| w.write_image_blob(&format))?;
            return Ok(Some(Bytes::from(vec)));
        }

        drop(entered);
        Ok(None) as Result<Option<Bytes>, UploadError>
    })
    .await?;

    Ok(opt)
}
