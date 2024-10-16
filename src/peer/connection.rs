use anyhow::{bail, Context};
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

use crate::peer::HandshakePacket;
use crate::PEER_ID;

#[derive(Debug, PartialEq)]
enum Frame {
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
    Request {
        // The zero-based piece index.
        index: u32,
        /// The zero-based byte offset within the piece
        /// This'll be 0 for the first block, 2^14 for the second block, 2*2^14
        /// for the third block etc.
        begin: u32,
        /// Generally a power of two unless it gets truncated by the end of the file.
        /// All current implementations use 2^14 (16 kiB), and close connections
        /// which request an amount greater than that
        length: u32,
    },

    /// Correlated with request messages implicitly. It is possible for an unexpected
    /// piece to arrive if choke and unchoke messages are sent in quick succession
    /// and/or transfer is going very slowly.
    Piece {
        // The zero-based piece index.
        index: u32,
        /// The zero-based byte offset within the piece.
        begin: u32,
        /// The data for the piece, usually 2^14 bytes long.
        chunk: Bytes,
    },

    Cancel {
        index: u32,
        begin: u32,
        length: u32,
    },
}

#[derive(Debug)]
pub struct Connection {
    pub stream: BufWriter<TcpStream>,
    buf: BytesMut,
}

const MAX: usize = 1 << 16;

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(stream),
            buf: BytesMut::with_capacity(32 * 1024),
        }
    }

    pub async fn handshake(&mut self, info_hash: [u8; 20]) -> crate::Result<HandshakePacket> {
        let mut packet = HandshakePacket::new(info_hash, *PEER_ID);
        self.stream.write_all(packet.as_bytes()).await?;
        self.stream.flush().await?;
        self.stream.read_exact(packet.as_bytes_mut()).await?;
        Ok(packet)
    }

    pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.decode()? {
                return Ok(Some(frame));
            }

            if 0 == self.stream.read_buf(&mut self.buf).await? {
                if self.buf.is_empty() {
                    return Ok(None);
                } else {
                    bail!("connection reset by peer")
                }
            }
        }

        // let len = self.stream.read_u32().await.context("read frame len")?;
        // let kind: Kind = self
        //     .stream
        //     .read_u8()
        //     .await
        //     .context("read fram kind")?
        //     .into();
        // let payload: Option<Box<[u8]>> = if len > 1 {
        //     self.buffer.resize(len as usize - 1, 0);
        //     self.stream
        //         .read_exact(&mut self.buffer)
        //         .await
        //         .context(format!("read frame payload: {len} {kind:?}"))?;
        //     Some(Box::from(self.buffer.as_slice()))
        // } else {
        //     None
        // };
        // Ok(Frame::new(len, kind, payload))
    }

    fn decode(&mut self) -> crate::Result<Option<Frame>> {
        if self.buf.len() < 4 {
            // Not enough data to read length marker.
            return Ok(None);
        }

        // Read length marker.
        let len = u32::from_be_bytes(
            self.buf[..4]
                .try_into()
                .context("parse frame length marker")?,
        ) as usize;

        if len == 0 {
            // `KeepAlive` messsage, skip length marker and continue parsing,
            // we may still have bytes left in the buffer.
            let _ = self.buf.get_u32(); // self.buf.advance(4);
            return self.decode();
        }

        if self.buf.len() < 4 + len {
            // The full data has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            self.buf.reserve(4 + len - self.buf.len());

            // We need more bytes to form the next frame.
            return Ok(None);
        }

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
            6 => Frame::Request {
                index: self.buf.get_u32(),
                begin: self.buf.get_u32(),
                length: self.buf.get_u32(),
            },
            7 => Frame::Piece {
                index: self.buf.get_u32(),
                begin: self.buf.get_u32(),
                chunk: self.buf.split_to(len - 9).freeze(),
            },
            8 => Frame::Cancel {
                index: self.buf.get_u32(),
                begin: self.buf.get_u32(),
                length: self.buf.get_u32(),
            },
            // TODO: Implemenet custom protocl error.
            n => bail!("protocol error; invalid message kind {n}"),
        };

        Ok(Some(frame))
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> crate::Result<()> {
        /* self.stream.write_u32(frame.len()).await?;
        self.stream.write_u8(frame.kind() as u8).await?;
        if let Some(bytes) = frame.payload() {
            self.stream.write_all(&bytes).await?;
        }
        self.stream.flush().await?; */
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
            Frame::Request {
                index,
                begin,
                length,
            } => {
                self.stream.write_u32(13).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_u32(*index).await?;
                self.stream.write_u32(*begin).await?;
                self.stream.write_u32(*length).await?;
            }
            Frame::Piece {
                index,
                begin,
                chunk,
            } => {
                self.stream.write_u32((9 + chunk.len()) as u32).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_u32(*index).await?;
                self.stream.write_u32(*begin).await?;
                self.stream.write_all(chunk).await?;
            }
            Frame::Cancel {
                index,
                begin,
                length,
            } => {
                self.stream.write_u32(13).await?;
                self.stream.write_u8(u8::from(frame)).await?;
                self.stream.write_u32(*index).await?;
                self.stream.write_u32(*begin).await?;
                self.stream.write_u32(*length).await?;
            }
            // `Choke`, `Unchoke`, `Interested`, and 'NotInterested' have no payload.
            frame => self.write_empty_frame(frame).await?,
        };

        self.stream.flush().await?;
        Ok(())
    }

    async fn write_empty_frame(&mut self, frame: &Frame) -> crate::Result<()> {
        self.stream.write_u32(1).await?;
        self.stream.write_u8(u8::from(frame)).await?;
        Ok(())
    }
}

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

#[allow(dead_code)]
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
pub struct PiecePacket<T: ?Sized = [u8]> {
    // The zero-based piece index.
    pub index: [u8; 4],

    /// The zero-based byte offset within the piece.
    pub begin: [u8; 4],

    /// The data for the piece, usually 2^14 bytes long.
    chunk: T,
}

#[allow(dead_code)]
impl PiecePacket {
    const LEAD: usize = std::mem::size_of::<PiecePacket<()>>();

    pub fn as_ref_from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() < Self::LEAD {
            return None;
        }
        let n = bytes.len();
        // NOTE:
        // We need the length part of the fat pointer to Piece to hold the length of _just_ the `block` field.
        // And the only way we can change the length of the fat pointer to Piece is by changing the
        // length of the fat pointer to the slice, which we do by slicing it. We can't slice it at
        // the front (as it would invalidate the ptr part of the fat pointer), so we slice it at
        // the back!

        /* let piece = &bytes[..n - Self::LEAD] as *const [u8] as *const PiecePacket;
        // Safety: Piece is a POD with repr(c), _and_ the fat pointer data length is the length of
        // the trailing DST field (thanks to the PIECE_LEAD offset).
        Some(unsafe { &*piece }); */

        unsafe {
            (std::ptr::slice_from_raw_parts(bytes.as_ptr(), n - Self::LEAD) as *const Self).as_ref()
        }
    }

    pub fn chunk(&self) -> &[u8] {
        &self.chunk
    }
}
