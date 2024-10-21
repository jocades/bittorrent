//! An implementation of the [BitTorrent Protocol](https://www.bittorrent.org/beps/bep_0003.html)

use std::ops::Deref;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio_util::bytes::{Buf, Bytes, BytesMut};

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

/// At any given time, a connection with a peer is in one of the below states.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// The connection has not yet been established or it had been connected
    /// before but has been stopped.
    Disconnected,
    /// The state during which the TCP connection is established.
    Connecting,
    /// The state after establishing the TCP connection and exchanging the
    /// initial BitTorrent handshake.
    Handshaking,
    /// This is the normal state of a peer session, in which any messages, apart
    /// from the 'handshake' and 'bitfield', may be exchanged.
    Connected,
}

/// The default and initial state of a peer session is `Disconnected`.
impl Default for State {
    fn default() -> Self {
        State::Disconnected
    }
}

/// A wrapper around the `TcpStream` to read and write framed messages.
#[derive(Debug)]
pub struct Connection {
    /// The `TcpStream` decorated with a `BufWriter` which provides buffered
    /// level buffering.
    stream: BufWriter<TcpStream>,
    /// The buffer for reading frames.
    buf: BytesMut,
}

/// 4B
const U32_SIZE: usize = std::mem::size_of::<u32>();

/// 65536B (64KiB)
const FRAME_MAX: usize = 1 << 16;

impl Connection {
    /// Create a new `Connection`, backed by `stream`. Read and write buffers
    /// are initialized.
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream: BufWriter::new(stream),
            buf: BytesMut::with_capacity(32 * 1024),
        }
    }

    /// Attempt to perform a handshake. This `Frame` is different from the rest,
    /// it is only used **once** at the beggining of the connection.
    ///
    /// Hence the use of a separate [`HandshakePacket`] type which is reused to
    /// allocate the handshake response.
    pub async fn handshake(&mut self, info_hash: Sha1Hash) -> Result<HandshakePacket> {
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

    /// Read a single `Frame` value from the underlying stream.
    ///
    /// Waits until it has retrieved enough data to parse a frame.
    /// Any data remaining in the read buffer after the frame has been parsed is
    /// kept there for the next call to `read_frame`.
    ///
    /// # Returns
    ///
    /// On success, the received frame is returned. If the `TcpStream` is
    /// closed in a way that doesn't break a frame in half, it returns `None`.
    /// Otherwise, an error is returned.
    pub async fn read(&mut self) -> crate::Result<Option<Frame>> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            // There is not enough buffered data to read a frame. Attempt to
            // read more data from the socket.
            //
            // On success, the number of bytes is returned. `0` indicates "end
            // of stream"
            if 0 == self.stream.read_buf(&mut self.buf).await? {
                if self.buf.is_empty() {
                    // The remote closed the connection. For this to be a clean
                    // shutdown, there should be no data in the read buffer. If
                    // there is, this means that the peer closed the socket while
                    // sending a frame
                    return Ok(None);
                } else {
                    Err(Error::ConnectionReset)?
                }
            }
        }
    }

    /// Tries to parse a frame from the buffer. If the buffer contains enough
    /// data, the frame is returned and the data removed from the buffer. If not
    /// enough data has been buffered yet, `Ok(None)` is returned. If the
    /// buffered data does not represent a valid frame, `Err` is returned
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

    /// Write a single `Frame` value to the underlying stream.
    ///
    /// The `Frame` value is written to the socket using the various `write_*`
    /// functions provided by `AsyncWrite`. Calling these functions directly on
    /// a `TcpStream` is **not** advised, as this will result in a large number of
    /// syscalls. However, it is fine to call these functions on a *buffered*
    /// write stream. The data will be written to the buffer. Once the buffer is
    /// full, it is flushed to the underlying socket.
    pub async fn write(&mut self, frame: &Frame) -> crate::Result<()> {
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
