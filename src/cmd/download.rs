use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, ensure};
use clap::Args;
use sha1::{Digest, Sha1};
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{trace, trace_span};

use crate::{download, Frame, Peer, Torrent};

#[derive(Args)]
pub struct Download {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
}

impl Download {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let tracker_info = torrent.discover().await?;

        let info_hash = torrent.info.hash()?;
        let pieces = torrent.pieces();
        let npieces = pieces.len();

        let (tx, mut rx) = mpsc::channel(100);
        let queue = Arc::new(Mutex::new((0..npieces).collect::<Vec<_>>()));

        let span = trace_span!("download");
        let _guard = span.enter();

        let mut tasks = Vec::with_capacity(tracker_info.peers.len());
        for addr in tracker_info.peers {
            let tx = tx.clone();
            let queue = queue.clone();

            let task = tokio::spawn(async move {
                let mut peer = Peer::connect(addr, info_hash).await?;
                trace!("connected to peer {addr}");

                let Some(Frame::Bitfield(_)) = peer.recv().await? else {
                    bail!("expected bitfield frame")
                };
                trace!("recv bitfield");

                peer.send(&Frame::Interested).await?;
                trace!("send interested");

                let Some(Frame::Unchoke) = peer.recv().await? else {
                    bail!("expected unchoke frame")
                };
                trace!("recv unchoke");

                while let Some(piece_index) = {
                    let mut guard = queue.lock().unwrap();
                    guard.pop()
                    // pieces_left.lock().unwrap().pop()
                } {
                    let piece_size = if piece_index == npieces - 1 {
                        let rest = torrent.info.len % torrent.info.plen;
                        if rest == 0 {
                            torrent.info.plen
                        } else {
                            rest
                        }
                    } else {
                        torrent.info.plen
                    };

                    trace!("piece");
                    let piece = download::piece(&mut peer, piece_index, piece_size).await?;
                    tx.send((piece_index, piece)).await?;
                    trace!("sent");
                }

                Ok(())
            });

            tasks.push(task);
        }

        let mut file = File::create(&self.output).await?;
        let mut completed = 0;

        while completed < npieces {
            if let Some((index, piece)) = rx.recv().await {
                ensure!(hex::encode(Sha1::digest(&piece)) == hex::encode(pieces[index]));

                file.seek(SeekFrom::Start((index * torrent.info.plen) as u64))
                    .await?;
                file.write_all(&piece).await?;

                completed += 1;
                eprintln!("Piece {index} completed. Progress: {completed}/{npieces}",);
            }
        }

        eprintln!("Download complete!");
        Ok(())
    }
}
