//! An implementation of the [BitTorrent Protocol](https://www.bittorrent.org/beps/bep_0003.html)

use std::ops::Deref;

use anyhow::Context;
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

use crate::peer::HandshakePacket;
use crate::{Chunk, Sha1Hash, CLIENT_ID};

#[derive(Debug, PartialEq)]
pub enum Frame {
    Choke,
    Unchoke,
    Interested,
    NotInterested,

    /// Single number, the index which that downloader just completed and
    /// checked the hash of.
    Have(u32),

    /// Only ever sent as the first message. Its payload is a bitfield with each
    /// index that downloader has sent set to one and the rest set to zero.
    /// Downloaders which don't have anything yet may skip the 'bitfield' message.
    /// The first byte of the bitfield corresponds to indices 0 - 7 from high bit
    /// to low bit, respectively. The next one 8-15, etc. Spare bits at the end
    /// are set to zero.
    Bitfield(Bytes),

    /// Request a piece chunk.
    Request(Request),

    /// Correlated with request messages implicitly. It is possible for an unexpected
    /// piece to arrive if choke and unchoke messages are sent in quick succession
    /// and/or transfer is going very slowly.
    Piece(Chunk),

    Cancel(Request),
}

#[derive(Debug, PartialEq)]
pub struct Request {
    // The zero-based piece index.
    pub index: u32,
    /// The zero-based byte offset within the piece
    /// This'll be 0 for the first block, 2^14 for the second block, 2*2^14
    /// for the third block etc.
    pub begin: u32,
    /// Generally a power of two unless it gets truncated by the end of the file.
    /// All current implementations use 2^14 (16 kiB), and close connections
    /// which request an amount greater than that
    pub length: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("protocol error; connection reset by peer")]
    ConnectionReset,
    #[error("protocol error; frame of length {0} is too large")]
    Overflow(usize),
    #[error("protocol error; unknown message kind: {0}")]
    UnknownKind(u8),
}

/// A wrapper around the `TcpStream` to read and write framed messages.
#[derive(Debug)]
pub struct Connection {
    stream: BufWriter<TcpStream>,
    buf: BytesMut,
}

/// 4B
const U32_SIZE: usize = std::mem::size_of::<u32>();

/// 65536B (64KiB)
const FRAME_MAX: usize = 1 << 16;

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(stream),
            buf: BytesMut::with_capacity(32 * 1024),
        }
    }

    pub async fn handshake(&mut self, info_hash: Sha1Hash) -> crate::Result<HandshakePacket> {
        let mut packet = HandshakePacket::new(info_hash, *CLIENT_ID);
        self.stream
            .write_all(packet.as_bytes())
            .await
            .context("write handshake packet")?;
        self.stream.flush().await?;
        self.stream
            .read_exact(packet.as_bytes_mut())
            .await
            .context("read handshake packet")?;
        Ok(packet)
    }

    pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            if 0 == self.stream.read_buf(&mut self.buf).await? {
                if self.buf.is_empty() {
                    return Ok(None);
                } else {
                    Err(Error::ConnectionReset)?
                }
            }
        }
    }

    fn parse_frame(&mut self) -> crate::Result<Option<Frame>> {
        if self.buf.len() < U32_SIZE {
            // Not enough data to read length marker.
            return Ok(None);
        }

        // Read length marker, this should not fail since we know we have 4 bytes in the buffer.
        let len = u32::from_be_bytes(self.buf[..4].try_into()?) as usize;
        if len == 0 {
            // `KeepAlive` messsage, skip length marker and continue parsing
            // since we may still have bytes left in the buffer.
            self.buf.advance(U32_SIZE);
            return self.parse_frame();
        }

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if len > FRAME_MAX {
            Err(Error::Overflow(len))?
        }

        if self.buf.len() < U32_SIZE + len {
            // The full data has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            self.buf.reserve(U32_SIZE + len - self.buf.len());

            // We need more bytes to form the next frame.
            return Ok(None);
        }

        // Skip length marker.
        self.buf.advance(U32_SIZE);

        let frame = match self.buf.get_u8() {
            0 => Frame::Choke,
            1 => Frame::Unchoke,
            2 => Frame::Interested,
            3 => Frame::NotInterested,
            4 => {
                let index = self.buf.get_u32();
                Frame::Have(index)
            }
            5 => {
                let bitfield = self.buf.split_to(len - 1).freeze();
                Frame::Bitfield(bitfield)
            }
            6 => Frame::Request(Request {
                index: self.buf.get_u32(),
                begin: self.buf.get_u32(),
                length: self.buf.get_u32(),
            }),
            7 => Frame::Piece(Chunk {
                piece_index: self.buf.get_u32() as usize,
                offset: self.buf.get_u32(),
                data: self.buf.split_to(len - 9).freeze(),
            }),
            8 => Frame::Cancel(Request {
                index: self.buf.get_u32(),
                begin: self.buf.get_u32(),
                length: self.buf.get_u32(),
            }),
            n => Err(Error::UnknownKind(n))?,
        };

        Ok(Some(frame))
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> crate::Result<()> {
        match frame {
            Frame::Have(index) => {
                self.stream.write_u32(5).await?;
                self.stream.write_u8(4).await?;
                self.stream.write_u32(*index).await?;
            }
            Frame::Bitfield(bitfield) => {
                self.stream.write_u32((1 + bitfield.len()) as u32).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_all(bitfield).await?;
            }
            Frame::Request(req) | Frame::Cancel(req) => {
                self.stream.write_u32(13).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_u32(req.index).await?;
                self.stream.write_u32(req.begin).await?;
                self.stream.write_u32(req.length).await?;
            }
            Frame::Piece(chunk) => {
                self.stream.write_u32((9 + chunk.data.len()) as u32).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_u32(chunk.piece_index as u32).await?;
                self.stream.write_u32(chunk.offset).await?;
                self.stream.write_all(&chunk.data).await?;
            }
            // `Choke`, `Unchoke`, `Interested`, and 'NotInterested' have no payload.
            frame => {
                self.stream.write_u32(1).await?;
                self.stream.write_u8(u8::from(frame)).await?;
            }
        };

        self.stream.flush().await?;
        Ok(())
    }
}

impl Deref for Connection {
    type Target = BufWriter<TcpStream>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

// It's feels kind of silly doesn't it?
impl From<&Frame> for u8 {
    fn from(value: &Frame) -> Self {
        use Frame::*;
        match value {
            Choke => 0,
            Unchoke => 1,
            Interested => 2,
            NotInterested => 3,
            Have(_) => 4,
            Bitfield(_) => 5,
            Request { .. } => 6,
            Piece { .. } => 7,
            Cancel { .. } => 8,
        }
    }
}
