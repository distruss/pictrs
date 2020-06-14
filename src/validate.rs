use crate::{config::Format, error::UploadError};
use actix_web::web;
use bytes::Bytes;
use image::{ImageDecoder, ImageEncoder, ImageFormat};
use std::io::Cursor;
use tracing::{debug, instrument, Span};

#[derive(Debug, thiserror::Error)]
pub(crate) enum GifError {
    #[error("Error decoding gif")]
    Decode(#[from] gif::DecodingError),

    #[error("Error reading bytes")]
    Io(#[from] std::io::Error),
}

// import & export image using the image crate
#[instrument(skip(bytes))]
pub(crate) async fn validate_image(
    bytes: Bytes,
    prescribed_format: Option<Format>,
) -> Result<(Bytes, mime::Mime), UploadError> {
    let span = Span::current();

    let tup = web::block(move || {
        let entered = span.enter();
        if let Some(prescribed) = prescribed_format {
            debug!("Load from memory");
            let img = image::load_from_memory(&bytes).map_err(UploadError::InvalidImage)?;
            debug!("Loaded");

            let mime = prescribed.to_mime();

            debug!("Writing");
            let mut bytes = Cursor::new(vec![]);
            img.write_to(&mut bytes, prescribed.to_image_format())?;
            debug!("Written");
            return Ok((Bytes::from(bytes.into_inner()), mime));
        }

        let format = image::guess_format(&bytes).map_err(UploadError::InvalidImage)?;

        debug!("Validating {:?}", format);
        let res = match format {
            ImageFormat::Png => Ok((validate_png(bytes)?, mime::IMAGE_PNG)),
            ImageFormat::Jpeg => Ok((validate_jpg(bytes)?, mime::IMAGE_JPEG)),
            ImageFormat::Bmp => Ok((validate_bmp(bytes)?, mime::IMAGE_BMP)),
            ImageFormat::Gif => Ok((validate_gif(bytes)?, mime::IMAGE_GIF)),
            _ => Err(UploadError::UnsupportedFormat),
        };
        debug!("Validated");
        drop(entered);
        res
    })
    .await?;

    Ok(tup)
}

#[instrument(skip(bytes))]
fn validate_png(bytes: Bytes) -> Result<Bytes, UploadError> {
    let decoder = image::png::PngDecoder::new(Cursor::new(&bytes))?;

    let mut bytes = Cursor::new(vec![]);
    let encoder = image::png::PNGEncoder::new(&mut bytes);
    validate_still_image(decoder, encoder)?;

    Ok(Bytes::from(bytes.into_inner()))
}

#[instrument(skip(bytes))]
fn validate_jpg(bytes: Bytes) -> Result<Bytes, UploadError> {
    let decoder = image::jpeg::JpegDecoder::new(Cursor::new(&bytes))?;

    let mut bytes = Cursor::new(vec![]);
    let encoder = image::jpeg::JPEGEncoder::new(&mut bytes);
    validate_still_image(decoder, encoder)?;

    Ok(Bytes::from(bytes.into_inner()))
}

#[instrument(skip(bytes))]
fn validate_bmp(bytes: Bytes) -> Result<Bytes, UploadError> {
    let decoder = image::bmp::BmpDecoder::new(Cursor::new(&bytes))?;

    let mut bytes = Cursor::new(vec![]);
    let encoder = image::bmp::BMPEncoder::new(&mut bytes);
    validate_still_image(decoder, encoder)?;

    Ok(Bytes::from(bytes.into_inner()))
}

#[instrument(skip(bytes))]
fn validate_gif(bytes: Bytes) -> Result<Bytes, GifError> {
    use gif::{Parameter, SetParameter};

    let mut decoder = gif::Decoder::new(Cursor::new(&bytes));

    decoder.set(gif::ColorOutput::Indexed);

    let mut reader = decoder.read_info()?;

    let width = reader.width();
    let height = reader.height();
    let global_palette = reader.global_palette().unwrap_or(&[]);

    let mut bytes = Cursor::new(vec![]);
    {
        let mut encoder = gif::Encoder::new(&mut bytes, width, height, global_palette)?;

        gif::Repeat::Infinite.set_param(&mut encoder)?;

        while let Some(frame) = reader.read_next_frame()? {
            debug!("Writing frame");
            encoder.write_frame(frame)?;
        }
    }

    Ok(Bytes::from(bytes.into_inner()))
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
