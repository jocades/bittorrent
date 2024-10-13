use std::{fs, path::PathBuf};

use clap::Args;
use sha1::{Digest, Sha1};

use crate::Torrent;

#[derive(Args)]
pub struct Info {
    path: PathBuf,
}

impl Info {
    pub fn execute(&self) -> crate::Result<()> {
        let bytes = fs::read(&self.path)?;
        let torrent: Torrent = serde_bencode::from_bytes(&bytes)?;

        let info_dict = serde_bencode::to_bytes(&torrent.info)?;
        let mut hasher = Sha1::new();
        hasher.update(&info_dict);
        let hash = hasher.finalize();

        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length);
        println!("Info Hash: {}", hex::encode(hash));

        Ok(())
    }
}
