use std::{net::SocketAddrV4, path::PathBuf};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufStream},
    net::TcpStream,
};

use clap::Args;

use crate::torrent::Torrent;

#[derive(Args)]
pub struct Handshake {
    path: PathBuf,
    addr: SocketAddrV4,
}

impl Handshake {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let stream = TcpStream::connect(&self.addr).await?;
        let mut stream = BufStream::new(stream);

        stream.write_u8(19).await?;
        stream.write_all(b"BitTorrent protocol").await?;
        stream.write_u64(0).await?;
        stream.write_all(&torrent.info.hash()?).await?;
        stream.write_all(b"jordi123456789abcdef").await?;
        stream.flush().await?;

        let len = 1 + 19 + 8 + 20 + 20;
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;
        let peer_id = &buf[48..];
        println!("Peer ID: {}", hex::encode(peer_id));

        Ok(())
    }
}
