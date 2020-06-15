use crate::{config::Format, error::UploadError, upload_manager::tmp_file};
use actix_web::web;
use image::{io::Reader, ImageDecoder, ImageEncoder, ImageFormat};
use rexiv2::{MediaType, Metadata};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
};
use tracing::{debug, instrument, trace, Span};

#[derive(Debug, thiserror::Error)]
pub(crate) enum GifError {
    #[error("Error decoding gif")]
    Decode(#[from] gif::DecodingError),

    #[error("Error reading bytes")]
    Io(#[from] std::io::Error),
}

// import & export image using the image crate
#[instrument]
pub(crate) async fn validate_image(
    tmpfile: PathBuf,
    prescribed_format: Option<Format>,
) -> Result<mime::Mime, UploadError> {
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
                validate(&tmpfile, ImageFormat::Jpeg)?;

                meta.clear();
                meta.save_to_file(&tmpfile)?;

                mime::IMAGE_JPEG
            }
            (Some(Format::Png), MediaType::Png) | (None, MediaType::Png) => {
                validate(&tmpfile, ImageFormat::Png)?;

                meta.clear();
                meta.save_to_file(&tmpfile)?;

                mime::IMAGE_PNG
            }
            (Some(Format::Jpeg), _) => {
                let newfile = tmp_file();
                convert(&tmpfile, &newfile, ImageFormat::Jpeg)?;

                mime::IMAGE_JPEG
            }
            (Some(Format::Png), _) => {
                let newfile = tmp_file();
                convert(&tmpfile, &newfile, ImageFormat::Png)?;

                mime::IMAGE_PNG
            }
            (_, MediaType::Bmp) => {
                let newfile = tmp_file();
                validate_bmp(&tmpfile, &newfile)?;

                mime::IMAGE_BMP
            }
            _ => return Err(UploadError::UnsupportedFormat),
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
fn validate_bmp(from: &PathBuf, to: &PathBuf) -> Result<(), UploadError> {
    debug!("Transmuting BMP");
    let decoder = image::bmp::BmpDecoder::new(BufReader::new(File::open(from)?))?;

    let mut writer = BufWriter::new(File::create(to)?);
    let encoder = image::bmp::BMPEncoder::new(&mut writer);
    validate_still_image(decoder, encoder)?;

    writer.flush()?;
    std::fs::rename(to, from)?;
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

fn validate_still_image<'a, D, E>(decoder: D, encoder: E) -> Result<(), UploadError>
where
    D: ImageDecoder<'a>,
    E: ImageEncoder,
{
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    let total_bytes = decoder.total_bytes();
    debug!("Reading image");
    let mut decoded_bytes = vec![0u8; total_bytes as usize];
    decoder.read_image(&mut decoded_bytes)?;

    debug!("Writing image");
    encoder.write_image(&decoded_bytes, width, height, color_type)?;

    Ok(())
}
