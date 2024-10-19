mod connection;
pub use connection::{Chunk, Connection, Frame, Request};

mod handshake;
pub use handshake::HandshakePacket;

use std::net::SocketAddrV4;

use anyhow::{bail, Result};
use tokio::net::TcpStream;

/// Hardcoded peer id for the challenge
pub const PEER_ID: &[u8; 20] = b"jordi123456789abcdef";

#[derive(Debug)]
pub struct Peer {
    conn: Connection,
}

impl Peer {
    /// Connect to a peer and try to perform a handshake to establish the connection.
    #[tracing::instrument(level = "trace", skip(info_hash))]
    pub async fn connect(addr: SocketAddrV4, info_hash: [u8; 20]) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);

        let handshake = conn.handshake(info_hash).await?;
        tracing::trace!("Peer ID: {}", hex::encode(handshake.peer_id()));

        Ok(Peer { conn })
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        self.conn.read_frame().await
    }

    #[tracing::instrument(level = "trace")]
    pub async fn request(&mut self, index: usize, begin: usize, length: usize) -> Result<Chunk> {
        self.send(&Frame::Request(Request {
            index: index as u32,
            begin: begin as u32,
            length: length as u32,
        }))
        .await?;

        let Some(Frame::Piece(chunk)) = self.recv().await? else {
            bail!("expected piece frame")
        };

        Ok(chunk)
    }
}
