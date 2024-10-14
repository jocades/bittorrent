use std::{net::SocketAddrV4, path::PathBuf};

use anyhow::Context;
use clap::Args;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufStream},
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
        let stream = TcpStream::connect(&self.addr).await?;
        let mut stream = BufStream::new(stream);

        let packet = HandshakePacket::new(torrent.info.hash()?, *PEER_ID);
        stream.write_all(packet.as_bytes()).await?;
        stream.flush().await?;

        let mut buf = [0u8; HandshakePacket::size()];
        stream.read_exact(&mut buf).await?;
        let res = HandshakePacket::from_bytes(&buf).context("parse handshake packet")?;
        println!("Peer ID: {}", hex::encode(res.peer_id()));

        Ok(())
    }
}
