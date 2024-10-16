use std::path::PathBuf;

use clap::Args;

use crate::Torrent;

#[derive(Args)]
pub struct Peers {
    path: PathBuf,
}

impl Peers {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let tracker_info = torrent.discover().await?;
        for addr in tracker_info.peers {
            println!("{addr}");
        }
        Ok(())
    }
}
