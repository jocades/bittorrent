//! Download flow:
//! -> handshake
//! <- handshake
//!
//! <- bitfield
//! -> interested
//! <- unchoke
//! -> request
//! <- piece
//!

//! This module is deemed to be dead by the end of the callenge.

#![allow(unused_imports)]
use std::io::SeekFrom;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, ensure, Result};
use bytes::{Bytes, BytesMut};
use sha1::{Digest, Sha1};
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::debug;

use crate::{tracker, Frame, Metainfo, Peer};

/// Max `piece chunk` size, 16 * 1024 bytes (16 kiB)
const CHUNK_MAX: usize = 1 << 14;

/* pub async fn full(torrent: &Metainfo, output: impl AsRef<Path>) -> Result<()> {
    download(torrent, output).await
} */

// Keep piece states in Mutex for fast updates, since contention is low.
// Use message passing for piece completion and complex operations.
// Consider atomics for progress tracking

/* #[derive(Debug)]
struct Piece {
    index: usize,
    size: usize,
    data: BytesMut,
} */

/* #[tracing::instrument(level = "trace", skip(torrent, output))]
async fn download(torrent: &Metainfo, output: impl AsRef<Path>) -> Result<()> {
    let peers = tracker::discover(&torrent).await?;

    // State that each worker needs.
    let info_hash = torrent.info.hash()?;
    let file_length = torrent.len();
    let piece_length = torrent.piece_len();
    let pieces = torrent.pieces();
    let npieces = pieces.len();

    let (tx, mut rx) = mpsc::channel::<(usize, BytesMut)>(100);
    let queue: Queue = Arc::new(Mutex::new((0..npieces).collect()));
    let mut tasks = Vec::with_capacity(peers.len());

    for addr in peers {
        let tx = tx.clone();
        let queue = queue.clone();

        let task = tokio::spawn(async move {
            let mut peer = match Peer::connect(addr, info_hash).await {
                Ok(peer) => peer,
                Err(why) => {
                    eprintln!("failed connecting to peer {addr}: {why}");
                    return Ok(());
                }
            };
            let Some(Frame::Bitfield(_)) = peer.recv().await? else {
                bail!("expected bitfield frame")
            };

            peer.send(&Frame::Interested).await?;

            if let Some(Frame::Unchoke) = peer.recv().await? {
                peer.chocked = false;
            } else {
                bail!("expected unchoke frame")
            };

            while let Some(piece_index) = {
                let mut queue = queue.lock().expect("acquire queue lock");
                queue.pop()
            } {
                // Check if last piece
                let piece_size = if piece_index == npieces - 1 {
                    let rest = file_length % piece_length;
                    if rest == 0 {
                        piece_length
                    } else {
                        rest
                    }
                } else {
                    piece_length
                };

                let piece = download_piece(&mut peer, piece_index, piece_size).await?;
                tx.send((piece_index, piece)).await?;
            }
            Ok(())
        });

        tasks.push(task);
    }

    let mut file = File::create(&output).await?;
    let mut completed = 0;

    while completed < npieces {
        match rx.recv().await {
            Some((index, piece)) => {
                ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[index]));

                file.seek(SeekFrom::Start((index * torrent.piece_len()) as u64))
                    .await?;
                file.write_all(&piece).await?;

                completed += 1;
                eprintln!("Piece {index} completed. Progress: {completed}/{npieces}",);
            }
            None => bail!("all workers have died"),
        }
    }

    for task in tasks {
        if let Err(e) = task.await? {
            eprintln!("download task failed: {e}");
        }
    }

    eprintln!("Download complete!");
    Ok(())
} */

pub async fn piece(peer: &mut Peer, piece_index: usize, size: usize) -> Result<BytesMut> {
    download_piece(peer, piece_index, size).await
}

/// Download a piece from a peer. In the future add suport do download a
/// a piece from multiple peers at the same time.
#[tracing::instrument(level = "trace", skip(peer))]
async fn download_piece(peer: &mut Peer, piece_index: usize, size: usize) -> Result<BytesMut> {
    let nchunks = (size + CHUNK_MAX - 1) / CHUNK_MAX; // round up for last chunk.
    let mut piece = BytesMut::with_capacity(size);

    for i in 0..nchunks {
        // Check if last chunk.
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

        let chunk = peer.request(piece_index, i * CHUNK_MAX, chunk_size).await?;

        ensure!(chunk.piece_index as usize == piece_index);
        ensure!(chunk.offset as usize == i * CHUNK_MAX);
        ensure!(chunk.data.len() == chunk_size);

        piece.extend(&chunk.data);
    }

    ensure!(piece.len() == size);

    Ok(piece)
}
