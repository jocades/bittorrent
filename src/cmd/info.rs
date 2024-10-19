use std::{fs, path::PathBuf};

use clap::Args;

use crate::Metainfo;

#[derive(Args)]
pub struct Info {
    path: PathBuf,
}

impl Info {
    pub fn execute(&self) -> crate::Result<()> {
        let bytes = fs::read(&self.path)?;
        let torrent = Metainfo::from_bytes(&bytes)?;
        let info_hash = torrent.info.hash()?;

        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.len());
        println!("Info Hash: {}", hex::encode(info_hash));
        println!("Piece Length: {}", torrent.piece_len());
        println!("Piece Hashes:");
        for hash in torrent.pieces() {
            println!("{}", hex::encode(hash));
        }

        Ok(())
    }
}
