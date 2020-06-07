use std::{net::SocketAddr, path::PathBuf};

#[derive(structopt::StructOpt)]
pub struct Config {
    #[structopt(
        short,
        long,
        help = "The address and port the server binds to, e.g. 127.0.0.1:80"
    )]
    addr: SocketAddr,

    #[structopt(short, long, help = "The path to the data directory, e.g. data/")]
    path: PathBuf,
}

impl Config {
    pub(crate) fn bind_address(&self) -> SocketAddr {
        self.addr
    }

    pub(crate) fn data_dir(&self) -> PathBuf {
        self.path.clone()
    }
}
