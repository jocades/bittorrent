use std::{net::SocketAddrV4, path::PathBuf};

use clap::Args;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::peer::HandshakePacket;
use crate::{Torrent, PEER_ID};

#[derive(Args)]
pub struct Handshake {
    path: PathBuf,
    addr: SocketAddrV4,
}

impl Handshake {
    pub async fn execute(&self) -> crate::Result<()> {
        let torrent = Torrent::read(&self.path)?;
        let mut stream = TcpStream::connect(&self.addr).await?;

        let mut packet = HandshakePacket::new(torrent.info.hash()?, *PEER_ID);
        stream.write_all(packet.as_bytes()).await?;

        stream.read_exact(packet.as_bytes_mut()).await?;
        println!("Peer ID: {}", hex::encode(packet.peer_id()));

        Ok(())
    }
}
