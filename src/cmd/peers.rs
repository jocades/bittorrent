use std::path::PathBuf;

use clap::Args;

use crate::{tracker, Metainfo};

#[derive(Args)]
pub struct Peers {
    path: PathBuf,
}

impl Peers {
    pub async fn execute(&self) -> crate::Result<()> {
        let meta = Metainfo::read(&self.path)?;
        let peers = tracker::discover(&meta).await?;
        for addr in peers {
            println!("{addr}");
        }
        Ok(())
    }
}
