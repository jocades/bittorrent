use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Args;
use tokio::net::TcpStream;

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

impl DownloadPiece {
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
        let Some(addr) = tracker_info.peers.first() else {
            bail!("empty peers list")
        };

        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);

        let handshake = conn.handshake(torrent.info.hash()?).await?;
        println!("Peer ID: {}", hex::encode(handshake.peer_id()));

        // recv: bitfield message
        let frame = conn.read_frame().await?;
        println!("{frame:?}");
        assert_eq!(frame.kind(), Kind::Bitfield);

        // send: interested
        let frame = Frame::with(Kind::Interested, None);
        conn.write_frame(&frame).await?;

        // recv: unchoke
        let frame = conn.read_frame().await?;
        println!("{frame:?}");
        assert_eq!(frame.kind(), Kind::Unchoke);

        let piece = torrent
            .pieces()
            .get(self.piece)
            .context(format!("piece not found: {}", self.piece))?;

        /*
        let piece = torrent.pieces()[0];
        let block = torrent.info.plen / (16 * 1024);
        println!("plen: {} block: {block}", torrent.info.plen);

        for (i, chunk) in torrent.pieces().chunks(16 * 1024).enumerate() {
            println!("chunk: {i} {}", chunk.len());
        } */

        Ok(())
    }
}
