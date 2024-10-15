use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};

use crate::peer::{frame::Kind, Frame, HandshakePacket};
use crate::PEER_ID;

#[derive(Debug)]
pub struct Connection {
    stream: BufWriter<TcpStream>,
    buffer: Vec<u8>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(stream),
            buffer: Vec::with_capacity(1024),
        }
    }

    pub async fn handshake(&mut self, info_hash: [u8; 20]) -> crate::Result<HandshakePacket> {
        let mut packet = HandshakePacket::new(info_hash, *PEER_ID);
        self.stream.write_all(packet.as_bytes()).await?;
        self.stream.flush().await?;
        self.stream.read_exact(packet.as_bytes_mut()).await?;
        Ok(packet)
    }

    pub async fn read_frame(&mut self) -> crate::Result<Frame> {
        let len = self.stream.read_u32().await?;
        let kind: Kind = self.stream.read_u8().await?.into();
        let payload: Option<Box<[u8]>> = if len > 1 {
            self.buffer.resize(len as usize - 1, 0);
            self.stream.read_exact(&mut self.buffer).await?;
            Some(Box::from(self.buffer.as_slice()))
        } else {
            None
        };
        Ok(Frame::new(len, kind, payload))
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> crate::Result<()> {
        self.stream.write_u32(frame.len()).await?;
        self.stream.write_u8(frame.kind() as u8).await?;
        if let Some(bytes) = frame.payload() {
            self.stream.write_all(&bytes).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }
}
