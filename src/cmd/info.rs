use std::{fs, path::PathBuf};

use clap::Args;

use crate::Torrent;

#[derive(Args)]
pub struct Info {
    path: PathBuf,
}

impl Info {
    pub fn execute(&self) -> crate::Result<()> {
        let bytes = fs::read(&self.path)?;
        let torrent = Torrent::from_bytes(&bytes)?;
        let info_hash = torrent.info.hash()?;

        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.len);
        println!("Info Hash: {}", hex::encode(info_hash));
        println!("Piece Length: {}", torrent.info.plen);
        println!("Piece Hashes:");
        for hash in torrent.pieces() {
            println!("{}", hex::encode(hash));
        }

        Ok(())
    }
}
