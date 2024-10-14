use std::path::PathBuf;

use anyhow::Context;
use clap::Args;

use crate::{
    torrent::{TrackerRequest, TrackerResponse},
    Torrent,
};

#[derive(Args)]
pub struct Peers {
    path: PathBuf,
}

impl Peers {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let info_hash = torrent.info_hash()?;

        let req = TrackerRequest {
            peer_id: "jordi123456789abcdef".into(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.len,
            compact: 1,
        };

        let encoded = info_hash
            .iter()
            .map(|byte| format!("%{:02x}", byte))
            .collect::<String>();
        let query = serde_urlencoded::to_string(&req).context("parse tracker request query")?;
        let url = format!("{}?info_hash={encoded}&{query}", torrent.announce);

        let bytes = reqwest::get(&url).await?.bytes().await?;
        let res: TrackerResponse = serde_bencode::from_bytes(&bytes)?;
        for peer in res.peers {
            println!("{peer}");
        }

        Ok(())
    }
}
