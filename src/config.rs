use std::{net::SocketAddr, path::PathBuf};

#[derive(structopt::StructOpt)]
pub(crate) struct Config {
    #[structopt(
        short,
        long,
        env = "PICTRS_ADDR",
        default_value = "0.0.0.0:8080",
        help = "The address and port the server binds to. Default: 0.0.0.0:8080"
    )]
    addr: SocketAddr,

    #[structopt(
      short, 
      long, 
      env = "PICTRS_PATH",
      help = "The path to the data directory, e.g. data/")]
    path: PathBuf,

    #[structopt(
        short,
        long,
        env = "PICTRS_FORMAT",
        help = "An image format to convert all uploaded files into, supports 'jpg' and 'png'"
    )]
    format: Option<Format>,
}

impl Config {
    pub(crate) fn bind_address(&self) -> SocketAddr {
        self.addr
    }

    pub(crate) fn data_dir(&self) -> PathBuf {
        self.path.clone()
    }

    pub(crate) fn format(&self) -> Option<Format> {
        self.format.clone()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid format supplied, {0}")]
pub(crate) struct FormatError(String);

#[derive(Clone, Debug)]
pub(crate) enum Format {
    Jpeg,
    Png,
}

impl Format {
    pub(crate) fn to_image_format(&self) -> image::ImageFormat {
        match self {
            Format::Jpeg => image::ImageFormat::Jpeg,
            Format::Png => image::ImageFormat::Png,
        }
    }

    pub(crate) fn to_mime(&self) -> mime::Mime {
        match self {
            Format::Jpeg => mime::IMAGE_JPEG,
            Format::Png => mime::IMAGE_PNG,
        }
    }
}

impl std::str::FromStr for Format {
    type Err = FormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(Format::Png),
            "jpg" => Ok(Format::Jpeg),
            other => Err(FormatError(other.to_string())),
        }
    }
}
