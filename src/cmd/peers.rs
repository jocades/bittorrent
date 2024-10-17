use std::path::PathBuf;

use clap::Args;

use crate::{tracker, Torrent};

#[derive(Args)]
pub struct Peers {
    path: PathBuf,
}

impl Peers {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let peers = tracker::discover(&torrent).await?;
        for addr in peers {
            println!("{addr}");
        }
        Ok(())
    }
}
