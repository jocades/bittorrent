use anyhow::Result;
use tokio::net::{TcpStream, ToSocketAddrs};
use tracing::debug;

mod connection;
pub use connection::{Connection, Frame};

mod handshake;
pub use handshake::HandshakePacket;

/// Hardcoded peer id for the challenge
pub const PEER_ID: &[u8; 20] = b"jordi123456789abcdef";

pub unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const u8, std::mem::size_of::<T>())
}

pub unsafe fn as_u8_slice_mut<T: Sized>(p: &mut T) -> &mut [u8] {
    std::slice::from_raw_parts_mut((p as *mut T) as *mut u8, std::mem::size_of::<T>())
}

#[derive(Debug)]
pub struct Peer {
    conn: Connection,
}

impl Peer {
    /// Connect to a peer and try to perform a handshake to establish the connection.
    pub async fn connect<T: ToSocketAddrs>(addr: T, info_hash: [u8; 20]) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);

        let handshake = conn.handshake(info_hash).await?;
        debug!("Peer ID: {}", hex::encode(handshake.peer_id()));

        Ok(Peer { conn })
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        self.conn.read_frame().await
    }
}
