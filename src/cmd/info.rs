use std::fs;

use clap::Args;

use crate::Torrent;

#[derive(Args)]
pub struct Info {
    // Add command-specific arguments here
}
// Tracker URL: http://bittorrent-test-tracker.codecrafters.io/announce
// Length: 92063
impl Info {
    pub fn execute(&self) -> crate::Result<()> {
        let bytes = fs::read("sample.torrent")?;
        let torrent: Torrent = serde_bencode::from_bytes(&bytes)?;
        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length);
        // Implement command logic here
        Ok(())
    }
}
