use crate::{config::Format, error::UploadError, upload_manager::tmp_file};
use actix_web::web;
use image::{io::Reader, ImageFormat};
use magick_rust::MagickWand;
use rexiv2::{MediaType, Metadata};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
};
use tracing::{debug, error, instrument, trace, warn, Span};

pub(crate) trait Op {
    fn op<F, T>(&self, f: F) -> Result<T, UploadError>
    where
        F: Fn(&Self) -> Result<T, &'static str>;

    fn op_mut<F, T>(&mut self, f: F) -> Result<T, UploadError>
    where
        F: Fn(&mut Self) -> Result<T, &'static str>;
}

impl Op for MagickWand {
    fn op<F, T>(&self, f: F) -> Result<T, UploadError>
    where
        F: Fn(&Self) -> Result<T, &'static str>,
    {
        match f(self) {
            Ok(t) => Ok(t),
            Err(e) => {
                if let Ok(e) = self.get_exception() {
                    error!("WandError: {}", e.0);
                    Err(UploadError::Wand(e.0.to_owned()))
                } else {
                    Err(UploadError::Wand(e.to_owned()))
                }
            }
        }
    }

    fn op_mut<F, T>(&mut self, f: F) -> Result<T, UploadError>
    where
        F: Fn(&mut Self) -> Result<T, &'static str>,
    {
        match f(self) {
            Ok(t) => Ok(t),
            Err(e) => {
                if let Ok(e) = self.get_exception() {
                    error!("WandError: {}", e.0);
                    Err(UploadError::Wand(e.0.to_owned()))
                } else {
                    Err(UploadError::Wand(e.to_owned()))
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GifError {
    #[error("Error decoding gif")]
    Decode(#[from] gif::DecodingError),

    #[error("Error reading bytes")]
    Io(#[from] std::io::Error),
}

pub(crate) fn image_webp() -> mime::Mime {
    "image/webp".parse().unwrap()
}

fn ptos(p: &PathBuf) -> Result<String, UploadError> {
    Ok(p.to_str().ok_or(UploadError::Path)?.to_owned())
}

// import & export image using the image crate
#[instrument]
pub(crate) async fn validate_image(
    tmpfile: PathBuf,
    prescribed_format: Option<Format>,
) -> Result<mime::Mime, UploadError> {
    let tmpfile_str = ptos(&tmpfile)?;
    let span = Span::current();

    let content_type = web::block(move || {
        let entered = span.enter();

        let meta = Metadata::new_from_path(&tmpfile)?;

        let content_type = match (prescribed_format, meta.get_media_type()?) {
            (_, MediaType::Gif) => {
                let newfile = tmp_file();
                validate_gif(&tmpfile, &newfile)?;

                mime::IMAGE_GIF
            }
            (Some(Format::Jpeg), MediaType::Jpeg) | (None, MediaType::Jpeg) => {
                {
                    let wand = MagickWand::new();
                    debug!("reading: {}", tmpfile_str);
                    wand.op(|w| w.read_image(&tmpfile_str))?;

                    debug!("format: {}", wand.op(|w| w.get_format())?);
                }

                meta.clear();
                meta.save_to_file(&tmpfile)?;

                mime::IMAGE_JPEG
            }
            (Some(Format::Png), MediaType::Png) | (None, MediaType::Png) => {
                {
                    let wand = MagickWand::new();
                    debug!("reading: {}", tmpfile_str);
                    wand.op(|w| w.read_image(&tmpfile_str))?;

                    debug!("format: {}", wand.op(|w| w.get_format())?);
                }

                meta.clear();
                meta.save_to_file(&tmpfile)?;

                mime::IMAGE_PNG
            }
            (Some(Format::Webp), MediaType::Other(webp)) | (None, MediaType::Other(webp))
                if webp == "image/webp" =>
            {
                let newfile = tmp_file();
                let newfile_str = ptos(&newfile)?;
                // clean metadata by writing new webp, since exiv2 doesn't support webp yet
                {
                    let wand = MagickWand::new();

                    debug!("reading: {}", tmpfile_str);
                    wand.op(|w| w.read_image(&tmpfile_str))?;

                    debug!("format: {}", wand.op(|w| w.get_format())?);
                    debug!("image_format: {}", wand.op(|w| w.get_image_format())?);
                    debug!("type: {}", wand.op(|w| Ok(w.get_type()))?);
                    debug!("image_type: {}", wand.op(|w| Ok(w.get_image_type()))?);

                    wand.op(|w| w.write_image(&newfile_str))?;
                }

                std::fs::rename(&newfile, &tmpfile)?;

                image_webp()
            }
            (Some(format), _) => {
                let newfile = tmp_file();
                let newfile_str = ptos(&newfile)?;
                {
                    let mut wand = MagickWand::new();

                    debug!("reading: {}", tmpfile_str);
                    wand.op(|w| w.read_image(&tmpfile_str))?;

                    debug!("format: {}", wand.op(|w| w.get_format())?);
                    debug!("image_format: {}", wand.op(|w| w.get_image_format())?);
                    debug!("type: {}", wand.op(|w| Ok(w.get_type()))?);
                    debug!("image_type: {}", wand.op(|w| Ok(w.get_image_type()))?);

                    wand.op_mut(|w| w.set_image_format(format.to_magick_format()))?;

                    debug!("writing: {}", newfile_str);
                    wand.op(|w| w.write_image(&newfile_str))?;
                }

                std::fs::rename(&newfile, &tmpfile)?;

                format.to_mime()
            }
            (_, media_type) => {
                warn!("Unsupported media type, {}", media_type);
                return Err(UploadError::UnsupportedFormat);
            }
        };

        drop(entered);
        Ok(content_type) as Result<mime::Mime, UploadError>
    })
    .await?;

    Ok(content_type)
}

#[instrument]
fn convert(from: &PathBuf, to: &PathBuf, format: ImageFormat) -> Result<(), UploadError> {
    debug!("Converting");
    let reader = Reader::new(BufReader::new(File::open(from)?)).with_guessed_format()?;

    if reader.format() != Some(format) {
        return Err(UploadError::UnsupportedFormat);
    }

    let img = reader.decode()?;

    img.save_with_format(to, format)?;
    std::fs::rename(to, from)?;
    Ok(())
}

#[instrument]
fn validate(path: &PathBuf, format: ImageFormat) -> Result<(), UploadError> {
    debug!("Validating");
    let reader = Reader::new(BufReader::new(File::open(path)?)).with_guessed_format()?;

    if reader.format() != Some(format) {
        return Err(UploadError::UnsupportedFormat);
    }

    reader.decode()?;
    Ok(())
}

#[instrument]
fn validate_gif(from: &PathBuf, to: &PathBuf) -> Result<(), GifError> {
    debug!("Transmuting GIF");
    use gif::{Parameter, SetParameter};

    let mut decoder = gif::Decoder::new(BufReader::new(File::open(from)?));

    decoder.set(gif::ColorOutput::Indexed);

    let mut reader = decoder.read_info()?;

    let width = reader.width();
    let height = reader.height();
    let global_palette = reader.global_palette().unwrap_or(&[]);

    let mut writer = BufWriter::new(File::create(to)?);
    let mut encoder = gif::Encoder::new(&mut writer, width, height, global_palette)?;

    gif::Repeat::Infinite.set_param(&mut encoder)?;

    while let Some(frame) = reader.read_next_frame()? {
        trace!("Writing frame");
        encoder.write_frame(frame)?;
    }

    drop(encoder);
    writer.flush()?;

    std::fs::rename(to, from)?;
    Ok(())
}
