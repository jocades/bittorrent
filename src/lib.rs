mod cmd;
pub use cmd::Command;

mod metainfo;
use metainfo::Metainfo;

mod tracker;

mod peer;
use peer::{Frame, Peer, PEER_ID};

mod download;

mod torrent;

pub type Result<T> = anyhow::Result<T>;

pub(crate) type Sha1Hash = [u8; 20];

#[derive(clap::Parser)]
#[command(version, author, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
