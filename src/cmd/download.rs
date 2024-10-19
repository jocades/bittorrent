use std::path::PathBuf;

use clap::Args;

use crate::{download, Torrent};

#[derive(Args, Debug)]
pub struct Download {
    path: PathBuf,
    #[arg(short)]
    output: PathBuf,
}

impl Download {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        download::full(&torrent, &self.output).await
    }
}
