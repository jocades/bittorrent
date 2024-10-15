use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use sha1::{Digest, Sha1};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tracing::{debug, debug_span, info, info_span, trace_span};

use crate::peer::RequestPacket;
use crate::peer::{frame::Kind, Connection, Frame};
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

        let span = trace_span!("download file");
        let _guard = span.enter();

        let piece_hashes = torrent.pieces();
        let mut file: Vec<u8> = Vec::with_capacity(torrent.info.len);

        let mut hashes = Vec::new();

        for (i, piece_hash) in piece_hashes.iter().enumerate() {
            let pspan = debug_span!("download piece");
            let _pguard = pspan.enter();

            // send: request
            let piece_size = if i == piece_hashes.len() - 1 {
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

            debug!(n_piece = i, ?piece_size, ?nchunks);

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

                let payload =
                    RequestPacket::new(i as u32, (i * CHUNK_MAX) as u32, chunk_size as u32);
                let req = Frame::with(Kind::Request, Some(payload.as_bytes().into()));
                conn.write_frame(&req).await?;

                info!(n_chunk = i, request = ?payload);

                // recv: piece
                let res = conn.read_frame().await?;
                let mut payload = res.payload().context("piece frame must have payload")?;
                assert_eq!(res.kind(), Kind::Piece);

                let index = payload.read_u32().await?;
                let begin = payload.read_u32().await?;
                let mut chunk = vec![0u8; chunk_size];
                payload.read_exact(&mut chunk).await.context("read chunk")?;

                info!(n_chunk = i, ?index, ?begin, chunk = chunk.len());

                assert_eq!(index as usize, i);
                assert_eq!(begin as usize, i * CHUNK_MAX);
                assert_eq!(chunk.len(), chunk_size);

                piece.extend(&chunk);
                debug!(filled = piece.len());
            }

            assert_eq!(piece.len(), piece_size);
            // assert_eq!(hex::encode(Sha1::digest(&piece)), hex::encode(piece_hash));
            let downloaded_piece_hash = hex::encode(Sha1::digest(&piece));
            info!(?downloaded_piece_hash, piece_hash = hex::encode(piece_hash));
            hashes.push(downloaded_piece_hash);

            file.extend(&piece);
            piece.clear();
        }

        assert_eq!(file.len(), torrent.info.len);

        // tokio::fs::write(&self.output, &file)
        //     .await
        //     .context("write piece")?;

        Ok(())
    }
}
