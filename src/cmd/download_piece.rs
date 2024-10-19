use std::path::PathBuf;

use anyhow::{bail, ensure, Context};
use clap::Args;
use sha1::{Digest, Sha1};

use crate::{download, tracker, Frame, Metainfo, Peer};

#[derive(Args)]
pub struct DownloadPiece {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
    piece: usize,
}

impl DownloadPiece {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Metainfo::read(&self.path)?;
        let info_hash = torrent.info.hash()?;
        let pieces = torrent.pieces();

        ensure!(self.piece < pieces.len());

        let peers = tracker::discover(&torrent).await?;
        let mut peer = Peer::connect(peers[0], info_hash).await?;

        let Some(Frame::Bitfield(_)) = peer.recv().await? else {
            bail!("expected bitfield frame")
        };

        peer.send(&Frame::Interested).await?;

        let Some(Frame::Unchoke) = peer.recv().await? else {
            bail!("expected unchoke frame")
        };

        let piece_size = if self.piece == pieces.len() - 1 {
            let rest = torrent.len() % torrent.piece_len();
            if rest == 0 {
                torrent.piece_len()
            } else {
                rest
            }
        } else {
            torrent.piece_len()
        };

        let piece = download::piece(&mut peer, self.piece, piece_size).await?;
        ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[self.piece]));

        tokio::fs::write(&self.output, &piece)
            .await
            .context("write piece")?;

        Ok(())
    }
}
