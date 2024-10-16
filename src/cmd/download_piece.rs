use std::path::PathBuf;

use anyhow::{bail, ensure, Context};
use clap::Args;
use sha1::{Digest, Sha1};

use crate::peer::{Frame, Peer};
use crate::torrent::{TrackerQuery, TrackerResponse};
use crate::Torrent;

#[derive(Args)]
pub struct DownloadPiece {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
    piece: usize,
}

/// Max `piece chunk` size, 16 * 1024 bytes (16 kiB)
const CHUNK_MAX: usize = 1 << 14;

impl DownloadPiece {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let pieces = torrent.pieces();
        ensure!(self.piece < pieces.len());
        let query = TrackerQuery {
            peer_id: "jordi123456789abcdef".into(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.len,
            compact: 1,
        };
        let url = torrent.url(&query)?;
        let bytes = reqwest::get(&url).await?.bytes().await?;
        let tracker_info: TrackerResponse = serde_bencode::from_bytes(&bytes)?;

        let addr = tracker_info.peers.first().context("empty peers list")?;
        let mut peer = Peer::connect(addr).await?;

        let handshake = peer.handshake(torrent.info.hash()?).await?;
        eprintln!("Peer ID: {}", hex::encode(handshake.peer_id()));

        // recv: bitfield message
        let Some(Frame::Bitfield(_)) = peer.recv().await? else {
            bail!("expected bitfield frame")
        };

        // send: interested
        peer.send(&Frame::Interested).await?;

        // recv: unchoke
        let Some(Frame::Unchoke) = peer.recv().await? else {
            bail!("expected unchoke frame")
        };

        // send: request
        let piece_size = if self.piece == pieces.len() - 1 {
            let rest = torrent.info.len % torrent.info.plen;
            if rest == 0 {
                torrent.info.plen
            } else {
                rest
            }
        } else {
            torrent.info.plen
        };

        let nchunks = (piece_size + CHUNK_MAX - 1) / CHUNK_MAX; // round up
        let mut piece: Vec<u8> = Vec::with_capacity(piece_size);

        for i in 0..nchunks {
            let chunk_size = if i == nchunks - 1 {
                let rest = piece_size % CHUNK_MAX;
                if rest == 0 {
                    CHUNK_MAX
                } else {
                    rest
                }
            } else {
                CHUNK_MAX
            };

            peer.send(&Frame::Request {
                index: self.piece as u32,
                begin: (i * CHUNK_MAX) as u32,
                length: chunk_size as u32,
            })
            .await?;

            // recv: piece
            let Some(Frame::Piece {
                index,
                begin,
                chunk,
            }) = peer.recv().await?
            else {
                bail!("expected piece frame")
            };

            assert_eq!(index, self.piece as u32);
            assert_eq!(begin as usize, i * CHUNK_MAX);
            assert_eq!(chunk.len(), chunk_size);

            piece.extend(&chunk);
        }

        assert_eq!(piece.len(), piece_size);
        assert_eq!(
            hex::encode(Sha1::digest(&piece)),
            hex::encode(pieces[self.piece])
        );

        tokio::fs::write(&self.output, &piece)
            .await
            .context("write piece")?;

        Ok(())
    }
}
