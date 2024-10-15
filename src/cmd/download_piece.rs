use std::path::PathBuf;

use anyhow::{ensure, Context};
use clap::Args;
use sha1::{Digest, Sha1};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tracing::debug;

use crate::peer::RequestPacket;
use crate::peer::{frame::Kind, Connection, Frame};
use crate::torrent::{TrackerQuery, TrackerResponse};
use crate::Torrent;

#[derive(Args)]
pub struct DownloadPiece {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
    piece: usize,
}

/// Max size piece chunk, 16 * 1024 bytes (16 kiB)
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
        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);

        let handshake = conn.handshake(torrent.info.hash()?).await?;
        eprintln!("Peer ID: {}", hex::encode(handshake.peer_id()));

        // recv: bitfield message
        let frame = conn.read_frame().await?;
        debug!(?frame);
        assert_eq!(frame.kind(), Kind::Bitfield);

        // send: interested
        let frame = Frame::with(Kind::Interested, None);
        conn.write_frame(&frame).await?;

        // recv: unchoke
        let frame = conn.read_frame().await?;
        debug!(?frame);
        assert_eq!(frame.kind(), Kind::Unchoke);

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

            let payload =
                RequestPacket::new(self.piece as u32, (i * CHUNK_MAX) as u32, chunk_size as u32);
            let req = Frame::with(Kind::Request, Some(payload.as_bytes().into()));
            conn.write_frame(&req).await?;

            // recv: piece
            let res = conn.read_frame().await?;
            let mut payload = res.payload().context("piece frame must have payload")?;
            assert_eq!(res.kind(), Kind::Piece);

            let index = payload.read_u32().await?;
            let begin = payload.read_u32().await?;
            let mut chunk = vec![0u8; chunk_size];
            payload.read_exact(&mut chunk).await.context("read chunk")?;

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
