use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{bail, ensure, Context};
use clap::Args;
use sha1::{Digest, Sha1};

use crate::{crafters, piece_picker::PiecePicker, torrent, tracker, Frame, Metainfo, Storage};

#[derive(Args)]
pub struct DownloadPiece {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
    piece: usize,
}

impl DownloadPiece {
    pub async fn execute(&self) -> crate::Result<()> {
        let meta = Metainfo::read(&self.path)?;
        let pieces = meta.pieces();

        // TODO: All of this is to comply with the callenge, it is ugly.
        let storage = Storage::new(&meta, "test_download");
        let piece_picker = PiecePicker::new(meta.piece_len());

        let shared = Arc::new(torrent::Shared {
            piece_picker: Mutex::new(piece_picker),
            info_hash: meta.info.hash().unwrap(),
            storage,
        });

        let (cmd_tx, _) = tokio::sync::mpsc::unbounded_channel();

        ensure!(self.piece < pieces.len());

        let resp = tracker::discover(&meta).await?;
        let mut peer = crafters::Peer::connect(resp.peers[0], shared, cmd_tx).await?;

        let Some(Frame::Bitfield(_)) = peer.recv().await? else {
            bail!("expected bitfield frame")
        };

        peer.send(&Frame::Interested).await?;

        let Some(Frame::Unchoke) = peer.recv().await? else {
            bail!("expected unchoke frame")
        };

        let piece_size = if self.piece == pieces.len() - 1 {
            let rest = meta.len() % meta.piece_len();
            if rest == 0 {
                meta.piece_len()
            } else {
                rest
            }
        } else {
            meta.piece_len()
        };

        let piece = crafters::piece(&mut peer, self.piece, piece_size).await?;
        ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[self.piece]));

        tokio::fs::write(&self.output, &piece)
            .await
            .context("write piece")?;

        Ok(())
    }
}
