mod connection;
pub use connection::{Connection, Frame, Request};

mod handshake;
pub use handshake::HandshakePacket;
use tracing::{info, instrument, trace};

use std::{net::SocketAddrV4, sync::Arc};

use anyhow::{bail, Result};
use tokio::{net::TcpStream, sync::mpsc};

use crate::torrent;
use crate::{Chunk, PieceIndex, Sha1Hash};

pub type Sender = mpsc::UnboundedSender<Command>;

/// The commands peer session can receive.
pub(crate) enum Command {
    /// The result of reading a block from disk.
    Chunk(Chunk),
    /// Notifies this peer session that a new piece is available.
    PieceCompletion(PieceIndex),
    /// Eventually shut down the peer session.
    Shutdown,
}

pub struct Peer {
    pub addr: SocketAddrV4,
    /// The framed connection.
    conn: Connection,
    /// The `Torrent` channel transmit.
    pub cmd_tx: torrent::Sender,
    /// The torrents shared state with the peers.
    pub shared: Arc<torrent::Shared>,
    /// Whether or not the remote peer has choked this client. When a peer chokes
    /// the client, it is a notification that no requests will be answered until
    /// the client is unchoked. The client should not attempt to send requests
    /// efor blocks, and it should consider all pending (unanswered) requests to
    /// be discarded by the remote peer.
    pub chocked: bool,
    /// Whether this peer is choking us.
    pub is_choking: bool,
    /// Whether or not the remote peer is interested in something this client
    /// has to offer. This is a notification that the remote peer will begin
    /// requesting blocks when the client unchokes them.
    pub interested: bool,
    /// Wether the peer is interested in a piece we have.
    pub is_interested: bool,
}

impl Peer {
    /// Connect to a peer and try to perform a handshake to establish the connection.
    #[instrument(level = "trace", skip(shared))]
    pub async fn connect(
        addr: SocketAddrV4,
        shared: Arc<torrent::Shared>,
        cmd_tx: torrent::Sender,
    ) -> Result<Self> {
        // TODO add timeouts.
        trace!("connecting to peer {addr}");
        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);
        trace!("connected to peer {addr}");

        trace!("sending handshake");
        let handshake = conn.handshake(shared.info_hash).await?;
        info!("Peer ID: {}", hex::encode(handshake.peer_id()));

        Ok(Peer {
            addr,
            conn,
            cmd_tx,
            shared,
            chocked: true,
            is_choking: true,
            interested: false,
            is_interested: false,
        })
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        self.conn.read_frame().await
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn request(&mut self, index: usize, begin: usize, length: usize) -> Result<Chunk> {
        let request = Request {
            index: index as u32,
            begin: begin as u32,
            length: length as u32,
        };
        trace!(?self.addr, ?request, "sending request");
        self.send(&Frame::Request(request)).await?;

        let Some(Frame::Piece(chunk)) = self.recv().await? else {
            bail!("expected piece frame")
        };

        Ok(chunk)
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn request_chunk(
        &mut self,
        index: usize,
        begin: usize,
        length: usize,
    ) -> Result<Chunk> {
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
