use tokio::net::{TcpStream, ToSocketAddrs};

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
    pub async fn connect<T: ToSocketAddrs>(addr: T) -> crate::Result<Self> {
        let socket = TcpStream::connect(addr).await?;
        let conn = Connection::new(socket);
        Ok(Peer { conn })
    }

    pub async fn handshake(&mut self, info_hash: [u8; 20]) -> crate::Result<HandshakePacket> {
        self.conn.handshake(info_hash).await
    }

    pub async fn send(&mut self, frame: &Frame) -> crate::Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> crate::Result<Option<Frame>> {
        self.conn.read_frame().await
    }
}

pub async fn download_piece(piece_hash: [u8; 20]) {
    todo!()
}
