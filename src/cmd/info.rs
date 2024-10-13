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
        let torrent = Torrent::from_bytes(&bytes)?;

        let info_dict = serde_bencode::to_bytes(&torrent.info)?;
        let info_hash = Sha1::digest(&info_dict);

        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length);
        println!("Info Hash: {}", hex::encode(info_hash));
        println!("Piece Length: {}", torrent.info.piece_length);
        println!("Piece Hashes:");
        for hash in torrent.pieces() {
            println!("{}", hex::encode(hash));
        }

        Ok(())
    }
}
