//! Download flow:
//! -> handshake
//! <- handshake
//!
//! <- bitfield
//! -> interested
//! <- unchoke
//! -> request
//! <- piece

use anyhow::{bail, ensure};
use bytes::BytesMut;

use crate::{Frame, Peer};

/// Max `piece chunk` size, 16 * 1024 bytes (16 kiB)
const CHUNK_MAX: usize = 1 << 14;

#[tracing::instrument(level = "trace", skip(peer))]
pub async fn piece(peer: &mut Peer, piece_index: usize, size: usize) -> crate::Result<BytesMut> {
    let nchunks = (size + CHUNK_MAX - 1) / CHUNK_MAX; // round up for last piece.
    let mut piece = BytesMut::with_capacity(size);

    for i in 0..nchunks {
        // Check if last piece.
        let chunk_size = if i == nchunks - 1 {
            let rest = size % CHUNK_MAX;
            if rest == 0 {
                CHUNK_MAX
            } else {
                rest
            }
        } else {
            CHUNK_MAX
        };

        peer.send(&Frame::Request {
            index: piece_index as u32,
            begin: (i * CHUNK_MAX) as u32,
            length: chunk_size as u32,
        })
        .await?;

        let Some(Frame::Piece {
            index,
            begin,
            chunk,
        }) = peer.recv().await?
        else {
            bail!("expected piece frame")
        };

        ensure!(index as usize == piece_index);
        ensure!(begin as usize == i * CHUNK_MAX);
        ensure!(chunk.len() == chunk_size);

        piece.extend(&chunk);
    }

    ensure!(piece.len() == size);

    Ok(piece)
}
