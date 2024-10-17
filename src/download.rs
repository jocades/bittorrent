//! Download flow:
//! -> handshake
//! <- handshake
//!
//! <- bitfield
//! -> interested
//! <- unchoke
//! -> request
//! <- piece

use std::io::SeekFrom;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, ensure, Result};
use bytes::BytesMut;
use sha1::{Digest, Sha1};
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::{tracker, Frame, Peer, Torrent};

/// Max `piece chunk` size, 16 * 1024 bytes (16 kiB)
const CHUNK_MAX: usize = 1 << 14;

type Queue = Arc<Mutex<Vec<usize>>>;

#[tracing::instrument(level = "trace", skip(torrent, output))]
pub async fn download(torrent: &Torrent, output: impl AsRef<Path>) -> Result<()> {
    let peers = tracker::discover(&torrent).await?;
    let info_hash = torrent.info.hash()?;
    let total_length = torrent.len();
    let piece_length = torrent.plen();
    let pieces = torrent.pieces();
    let npieces = pieces.len();

    let (tx, mut rx) = mpsc::channel::<(usize, BytesMut)>(100);
    let queue: Queue = Arc::new(Mutex::new((0..npieces).collect()));
    let mut tasks = Vec::with_capacity(peers.len() /* TODO: make configurable */);

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

            let Some(Frame::Unchoke) = peer.recv().await? else {
                bail!("expected unchoke frame")
            };

            while let Some(piece_index) = {
                let mut guard = queue.lock().expect("acquire queue lock");
                guard.pop()
            } {
                let piece_size = if piece_index == npieces - 1 {
                    let rest = total_length % piece_length;
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
        if let Some((index, piece)) = rx.recv().await {
            ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[index]));

            file.seek(SeekFrom::Start((index * torrent.plen()) as u64))
                .await?;
            file.write_all(&piece).await?;

            completed += 1;
            eprintln!("Piece {index} completed. Progress: {completed}/{npieces}",);
        }
    }

    for task in tasks {
        if let Err(e) = task.await? {
            eprintln!("download task failed: {e}");
        }
    }

    eprintln!("Download complete!");
    Ok(())
}

#[tracing::instrument(level = "trace", skip(peer))]
pub async fn download_piece(peer: &mut Peer, piece_index: usize, size: usize) -> Result<BytesMut> {
    let nchunks = (size + CHUNK_MAX - 1) / CHUNK_MAX; // round up for last piece.
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
