use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};

use crate::peer::{frame::Kind, Frame, HandshakePacket};
use crate::PEER_ID;

#[derive(Debug)]
pub struct Connection {
    pub stream: BufWriter<TcpStream>,
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
        let len = self.stream.read_u32().await.context("read frame len")?;
        let kind: Kind = self
            .stream
            .read_u8()
            .await
            .context("read fram kind")?
            .into();
        let payload: Option<Box<[u8]>> = if len > 1 {
            self.buffer.resize(len as usize - 1, 0);
            self.stream
                .read_exact(&mut self.buffer)
                .await
                .context(format!("read frame payload: {len} {kind:?}"))?;
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

use super::{as_u8_slice, as_u8_slice_mut};

#[repr(C, packed)]
#[derive(Debug)]
pub struct RequestPacket {
    // The zero-based piece index
    index: [u8; 4],

    /// The zero-based byte offset within the piece
    /// This'll be 0 for the first block, 2^14 for the second block, 2*2^14 for the third block etc.
    /// length: the length of the block in bytes
    begin: [u8; 4],

    /// This'll be 2^14 (16 * 1024) for all blocks except the last one.
    /// The last block will contain 2^14 bytes or less, you'll need calculate this value using the piece length.
    length: [u8; 4],
}

impl RequestPacket {
    pub fn new(index: u32, begin: u32, length: u32) -> Self {
        Self {
            index: u32::to_be_bytes(index),
            begin: u32::to_be_bytes(begin),
            length: u32::to_be_bytes(length),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { as_u8_slice(self) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { as_u8_slice_mut(self) }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PiecePacket {
    // The zero-based piece index.
    pub index: [u8; 4],

    /// The zero-based byte offset within the piece.
    pub begin: [u8; 4],

    /// The data for the piece, usually 2^14 bytes long.
    block: Box<[u8]>,
}

#[allow(dead_code)]
impl PiecePacket {
    pub fn new(index: u32, begin: u32, block: &[u8]) -> Self {
        Self {
            index: u32::to_be_bytes(index),
            begin: u32::to_be_bytes(begin),
            block: block.into(),
        }
    }

    pub fn block(&self) -> &[u8] {
        &self.block
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { as_u8_slice(self) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { as_u8_slice_mut(self) }
    }

    #[allow(dead_code)]
    pub fn as_ref_from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() != std::mem::size_of::<Self>() {
            return None;
        }
        // SAFETY:
        // - We have checked that `bytes.len()` equals `size_of::<Self>()`, ensuring we are not over-reading.
        // - We are copying into a properly aligned and sized instance of `Self`.
        unsafe {
            (std::ptr::slice_from_raw_parts(bytes.as_ptr(), std::mem::size_of::<Self>())
                as *const Self)
                .as_ref()
        }
    }
}
