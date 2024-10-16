use std::path::PathBuf;

use anyhow::{bail, ensure, Context};
use clap::Args;
use sha1::{Digest, Sha1};

use crate::torrent::{TrackerQuery, TrackerResponse};
use crate::{download, Frame, Peer, Torrent};

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
        let info_hash = torrent.info.hash()?;
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
        let mut peer = Peer::connect(addr, info_hash).await?;

        let Some(Frame::Bitfield(_)) = peer.recv().await? else {
            bail!("expected bitfield frame")
        };

        peer.send(&Frame::Interested).await?;

        let Some(Frame::Unchoke) = peer.recv().await? else {
            bail!("expected unchoke frame")
        };

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

        let piece = download::piece(&mut peer, self.piece, piece_size).await?;
        ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[self.piece]));

        tokio::fs::write(&self.output, &piece)
            .await
            .context("write piece")?;

        Ok(())
    }
}
