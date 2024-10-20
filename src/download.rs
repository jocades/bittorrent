use bytes::{Bytes, BytesMut};

use crate::PieceIndex;

/// This is the only chunk size we're dealing with (except for possibly the
/// last chunk).  It is the widely used and accepted 16 KiB.
pub const CHUNK_MAX: usize = 1 << 14;

/// A chunk is a fixed size part of a piece, which in turn is a fixed size
/// part of a torrent. Downloading torrents happen at this chunk level
/// granularity.
#[derive(Debug, PartialEq)]
pub struct Chunk {
    // The zero-based piece index.
    pub piece_index: PieceIndex,
    /// The zero-based byte offset within the piece.
    pub offset: u32,
    /// The data of the chunk, usually 2^14 bytes long.
    pub data: Bytes,
}

/// Returns the number of chunks in a piece of the given length.
pub(crate) fn chunk_count(piece_size: usize) -> usize {
    // all but the last piece are a multiple of the block length, but the
    // last piece may be shorter so we need to account for this by rounding
    // up before dividing to get the number of blocks in piece
    (piece_size + CHUNK_MAX - 1) / CHUNK_MAX
}

#[derive(Debug)]
pub(crate) struct PieceDownload {
    pub index: PieceIndex,
    /// All the chunks of the piece.
    pub data: BytesMut,
}
