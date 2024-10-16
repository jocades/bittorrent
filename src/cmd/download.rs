use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Args;
use sha1::{Digest, Sha1};
use tokio::net::TcpStream;
use tracing::{debug, debug_span, info, info_span, trace_span};

use crate::peer::{Connection, Frame};
use crate::torrent::{TrackerQuery, TrackerResponse};
use crate::Torrent;

#[derive(Args)]
pub struct Download {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
}

const CHUNK_MAX: usize = 1 << 14;

impl Download {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
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
        let frame = conn.read_frame().await?.context("read bitfield")?;
        debug!(?frame);
        assert!(matches!(frame, Frame::Bitfield(_)));

        // send: interested
        // let frame = Frame::with(Kind::Interested, None);
        // conn.send(&frame).await?;
        conn.write_frame(&Frame::Interested).await?;

        // recv: unchoke
        let frame = conn.read_frame().await?.context("read unchoke")?;
        debug!(?frame);
        assert!(matches!(frame, Frame::Unchoke));

        let span = trace_span!("download file");
        let _guard = span.enter();

        let piece_hashes = torrent.pieces();
        let mut file: Vec<u8> = Vec::with_capacity(torrent.info.len);

        // let mut hashes = Vec::new();

        for (piece_idx, piece_hash) in piece_hashes.iter().enumerate() {
            let pspan = debug_span!("download piece");
            let _pguard = pspan.enter();

            // send: request
            let piece_size = if piece_idx == piece_hashes.len() - 1 {
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

            debug!(?piece_idx, ?piece_size, ?nchunks);

            for i in 0..nchunks {
                let cspan = info_span!("download chunk");
                let _cguard = cspan.enter();

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

                info!(n_chunk = i, ?chunk_size);

                let request = Frame::Request {
                    index: piece_idx as u32,
                    begin: (i * CHUNK_MAX) as u32,
                    length: chunk_size as u32,
                };
                conn.write_frame(&request).await?;

                info!(n_chunk = i, ?request);

                // recv: piece
                // recv: piece
                let Frame::Piece {
                    index,
                    begin,
                    chunk,
                } = conn.read_frame().await?.context("read piece")?
                else {
                    bail!("expected piece frame")
                };

                info!(n_chunk = i, ?index, ?begin, chunk = chunk.len());

                assert_eq!(index as usize, i);
                assert_eq!(begin as usize, i * CHUNK_MAX);
                assert_eq!(chunk.len(), chunk_size);

                piece.extend(&chunk);
                debug!(filled = piece.len());
            }

            assert_eq!(piece.len(), piece_size);
            assert_eq!(hex::encode(Sha1::digest(&piece)), hex::encode(piece_hash));

            file.extend(&piece);
            piece.clear();
        }

        assert_eq!(file.len(), torrent.info.len);

        tokio::fs::write(&self.output, &file)
            .await
            .context("write piece")?;

        Ok(())
    }
}
