use std::path::PathBuf;

use clap::Args;
use tokio_util::sync::CancellationToken;

#[allow(unused_imports)]
use crate::{torrent, Metainfo, Torrent};

#[derive(Args, Debug)]
pub struct Download {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
}

impl Download {
    pub async fn execute(&self) -> crate::Result<()> {
        let meta = Metainfo::read(&self.path)?;
        let mut torrent = Torrent::new(meta, torrent::Conf::default(), CancellationToken::new());
        torrent.run().await?;
        // torrent::run(&self.path, signal::ctrl_c()).await;
        Ok(())
    }
}
