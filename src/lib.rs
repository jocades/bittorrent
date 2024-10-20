mod cmd;
pub use cmd::Command;

mod metainfo;
use metainfo::Metainfo;

mod tracker;
#[allow(unused_imports)]
use tracker::Tracker;

mod peer;
use peer::{Frame, Peer};

mod crafters;

mod torrent;
use torrent::{Chunk, Torrent};

/// The application protocol.
pub const PROTOCOL: &[u8; 19] = b"BitTorrent protocol";

/// Hardcoded client id for the challenge (the current users `peer id`).
pub const CLIENT_ID: &[u8; 20] = b"jordi123456789abcdef";

pub type Result<T> = anyhow::Result<T>;

/// The type of a piece's index.
///
/// Over the wire all integers are sent as 4-byte big endian integers.
pub(crate) type PieceIndex = usize;

/// The peer ID is an arbitrary 20 byte string.
///
/// Guidelines for choosing a peer ID: http://bittorrent.org/beps/bep_0020.html.
pub type PeerId = [u8; 20];

/// Represnts the a Sha1Hash returned by the Sha1 crate, since it uses old
/// rust's `GenericArray`.
pub(crate) type Sha1Hash = [u8; 20];

#[derive(clap::Parser)]
#[command(version, author, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
