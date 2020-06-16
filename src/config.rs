use std::{collections::HashSet, net::SocketAddr, path::PathBuf};

#[derive(Clone, Debug, structopt::StructOpt)]
pub(crate) struct Config {
    #[structopt(
        short,
        long,
        help = "Whether to skip validating images uploaded via the internal import API"
    )]
    skip_validate_imports: bool,

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
        help = "The path to the data directory, e.g. data/"
    )]
    path: PathBuf,

    #[structopt(
        short,
        long,
        env = "PICTRS_FORMAT",
        help = "An optional image format to convert all uploaded files into, supports 'jpg', 'png', and 'webp'"
    )]
    format: Option<Format>,

    #[structopt(
        short,
        long,
        env = "PICTRS_FILTER_WHITELIST",
        help = "An optional list of filters to whitelist, supports 'identity', 'thumbnail', and 'blur'"
    )]
    whitelist: Option<Vec<String>>,

    #[structopt(
        short,
        long,
        env = "PICTRS_MAX_FILE_SIZE",
        help = "Specify the maximum allowed uploaded file size (in Megabytes)",
        default_value = "40"
    )]
    max_file_size: usize,
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

    pub(crate) fn filter_whitelist(&self) -> Option<HashSet<String>> {
        self.whitelist
            .as_ref()
            .map(|wl| wl.iter().cloned().collect())
    }

    pub(crate) fn validate_imports(&self) -> bool {
        !self.skip_validate_imports
    }

    pub(crate) fn max_file_size(&self) -> usize {
        self.max_file_size
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid format supplied, {0}")]
pub(crate) struct FormatError(String);

#[derive(Clone, Debug)]
pub(crate) enum Format {
    Jpeg,
    Png,
    Webp,
}

impl Format {
    pub(crate) fn to_mime(&self) -> mime::Mime {
        match self {
            Format::Jpeg => mime::IMAGE_JPEG,
            Format::Png => mime::IMAGE_PNG,
            Format::Webp => "image/webp".parse().unwrap(),
        }
    }

    pub(crate) fn to_magick_format(&self) -> &'static str {
        match self {
            Format::Jpeg => "JPEG",
            Format::Png => "PNG",
            Format::Webp => "WEBP",
        }
    }
}

impl std::str::FromStr for Format {
    type Err = FormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(Format::Png),
            "jpg" => Ok(Format::Jpeg),
            "webp" => Ok(Format::Webp),
            other => Err(FormatError(other.to_string())),
        }
    }
}
