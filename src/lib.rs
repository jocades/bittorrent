mod cmd;
pub use cmd::Command;

mod metainfo;
use metainfo::Metainfo;

mod tracker;

mod peer;
use peer::{Frame, Peer};

mod download;

mod torrent;
use torrent::{Chunk, Torrent};

/// Hardcoded client id for the challenge (the current users `peer id`).
pub const CLIENT_ID: &[u8; 20] = b"jordi123456789abcdef";

pub type Result<T> = anyhow::Result<T>;

/// The type of a piece's index.
///
/// On the wire all integers are sent as 4-byte big endian integers.
pub(crate) type PieceIndex = usize;

/// Represnts the a Sha1Hash returned by the Sha1 crate, since it uses old
/// rusts `GenericArray`.
pub(crate) type Sha1Hash = [u8; 20];

#[derive(clap::Parser)]
#[command(version, author, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
