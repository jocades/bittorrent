use std::path::PathBuf;

use clap::Args;

use crate::{
    torrent::{TrackerQuery, TrackerResponse},
    Torrent,
};

#[derive(Args)]
pub struct Peers {
    path: PathBuf,
}

impl Peers {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let query = TrackerQuery {
            peer_id: "jordi123456789abcdef".into(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.len,
            compact: 1,
        };
        let url = torrent.url(&query)?;
        let bytes = reqwest::get(&url).await?.bytes().await?;
        let res: TrackerResponse = serde_bencode::from_bytes(&bytes)?;
        for addr in res.peers {
            println!("{addr}");
        }

        Ok(())
    }
}
