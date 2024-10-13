use std::{fs, path::PathBuf};

use anyhow::bail;
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
        let info_hash = hasher.finalize();

        if torrent.info.pieces.len() % 20 != 0 {
            bail!("pieces must be divisible by 20")
        }

        let piece_hashes = torrent
            .info
            .pieces
            .chunks(20)
            .map(|chunk| hex::encode(chunk))
            .collect::<Vec<String>>()
            .join("\n");

        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length);
        println!("Info Hash: {}", hex::encode(info_hash));
        println!("Piece Length: {}", torrent.info.piece_length);
        println!("Piece Hashes:\n{}", piece_hashes);

        Ok(())
    }
}
