#![allow(dead_code)]
use std::{collections::VecDeque, net::SocketAddrV4, path::PathBuf};

use bytes::Bytes;

use crate::{metainfo::Metainfo, PieceIndex};

#[derive(Debug)]
struct PieceDownload {
    index: PieceIndex,
    size: usize,
    chunks: Vec<Chunk>,
}

struct PeerSession {
    // tx:
}

struct Conf {
    /// The max number of connected peers the torrent should have.
    max_connections: usize,
}

impl Default for Conf {
    fn default() -> Self {
        Self {
            // This value is mostly picked for performance while keeping in mind
            // not to overwhelm the host.
            max_connections: 50,
        }
    }
}

#[derive(Clone)]
enum PieceState {
    Missing,
    Downloading(usize),
    Complete,
}

struct Storage {
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
    pub fn new(meta: &Metainfo, dest: PathBuf) -> Self {
        let total_size = meta.len();
        let piece_count = meta.pieces().len();
        let piece_size = meta.piece_len();
        let last_piece_size = total_size - piece_size * (piece_count - 1);

        Storage {
            total_size,
            piece_count: meta.pieces().len(),
            piece_size: meta.piece_len(),
            last_piece_size,
            dest,
        }
    }
}

/// Decides which piece is more suitable to download next. At the moment it is
/// just a queue.
struct PiecePicker {
    /// For now just hold a vec and access by piece index, implement better
    /// strategies in the feature to do quick lookups of which pieces we have,
    /// which pieces we are are currently intersted in or wich we are serving.
    pieces: Vec<PieceState>,
    storage: Storage,
}

struct Piece {
    state: PieceState,
    size: usize,
}

impl PiecePicker {
    pub fn new(length: usize, storage: Storage) -> Self {
        PiecePicker {
            pieces: vec![PieceState::Missing; length],
            storage,
        }
    }

    pub fn pick(&mut self) -> PieceIndex {
        todo!()
    }
}

#[derive(Debug, PartialEq)]
pub struct Chunk {
    // The zero-based piece index.
    pub index: u32,
    /// The zero-based byte offset within the piece.
    pub begin: u32,
    /// The data for the piece, usually 2^14 bytes long.
    pub data: Bytes,
}

/// The orchestrator of a torrent download or upload.
pub struct Torrent {
    /// The metainfo for this torrent.
    metainfo: Metainfo,
    /// The peers currently in this torrent.
    peers: SocketAddrV4,
    /// The peers returned by the tracker which we can connect.
    available_peers: Vec<SocketAddrV4>,
}
