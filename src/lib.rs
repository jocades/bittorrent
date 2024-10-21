mod cmd;
use cmd::Command;

mod metainfo;
use metainfo::Metainfo;

mod tracker;
#[allow(unused_imports)]
use tracker::Tracker;

mod peer;
use peer::Frame;

mod crafters;

mod download;
use download::{Chunk, PieceDownload};

mod torrent;
use torrent::Torrent;

mod piece_picker;
use piece_picker::PiecePicker;

mod disk;

use std::path::{Path, PathBuf};

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

#[allow(dead_code)]
pub(crate) struct Storage {
    total_size: usize,
    piece_count: usize,
    piece_size: usize,
    last_piece_size: usize,
    /// The download destination directory of the torrent.
    ///
    /// In case of single file downloads, this is the directory where the file
    /// is downloaded, named as the torrent.
    /// In case of archive downloads, this directory is the download directory
    /// joined by the torrent's name.
    dest: PathBuf,
}

impl Storage {
    /// Extracts storage related information from the torrent metainfo.
    pub fn new(meta: &Metainfo, dest: impl AsRef<Path>) -> Self {
        let total_size = meta.len();
        let piece_size = meta.piece_len();
        let piece_count = meta.pieces().len();
        let last_piece_size = total_size - piece_size * (piece_count - 1);
        tracing::trace!(
            %total_size,
            %piece_size,
            %piece_count,
            %last_piece_size,
            "storage"
        );

        Storage {
            total_size,
            piece_count,
            piece_size,
            last_piece_size,
            dest: dest.as_ref().into(),
        }
    }

    /// Returns the size of the piece at the given index.
    ///
    /// # Panics
    ///
    /// If the piece index is out of range. This should never happen inside the
    /// system since everything works on the assumption that piece indices are
    /// always valid.
    pub fn piece_size(&self, index: PieceIndex) -> usize {
        assert!(index < self.piece_count, "piece index out of bounds");
        if index == self.piece_count - 1 {
            self.last_piece_size
        } else {
            self.piece_size
        }
    }
}

#[derive(clap::Parser)]
#[command(version, author, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
