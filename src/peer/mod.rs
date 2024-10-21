mod connection;
pub use connection::{Connection, Frame, Request};

mod handshake;
pub use handshake::HandshakePacket;
use tokio_util::bytes::BytesMut;
use tracing::{debug, info, instrument, trace};

use std::{net::SocketAddrV4, sync::Arc};

use anyhow::{bail, ensure, Result};
use tokio::{
    net::{unix::SocketAddr, TcpStream},
    sync::mpsc,
    task,
    time::{self, Duration},
};

use crate::download::{chunk_count, PieceDownload, CHUNK_MAX};
use crate::{torrent, Chunk, PieceIndex};

#[allow(dead_code)]
pub type Sender = mpsc::UnboundedSender<Command>;

type Receiver = mpsc::UnboundedReceiver<Command>;

/// The commands peer session can receive.
#[allow(dead_code)]
pub(crate) enum Command {
    /// The result of reading a block from disk.
    Chunk(Chunk),
    /// Notifies this peer session that a new piece is available.
    PieceCompletion(PieceIndex),
    /// Eventually shut down the peer session.
    Shutdown,
}

struct State {
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

pub struct Session {
    pub addr: SocketAddrV4,
    /// The framed connection.
    pub conn: Connection,

    tx: Sender,
    rx: Receiver,

    /// The [`Torrent`] shared state with the peers.
    pub torrent: Arc<torrent::Shared>,
    /// The [`Torrent`] transmit cannel.
    pub cmd_tx: torrent::Sender,
    /// The peers state.
    pub state: State,
}

impl Session {
    pub async fn outbound(
        addr: SocketAddrV4,
        shared: Arc<torrent::Shared>,
        cmd_tx: torrent::Sender,
        rx: Receiver,
    ) -> Result<(Self, Sender)> {
        // TODO: add timeouts.
        trace!("connecting");
        let (tx, rx) = mpsc::unbounded_channel();
        let sender = tx.clone();

        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);
        trace!("connected");

        trace!("send handshake");
        let handshake = conn.handshake(shared.info_hash).await?;
        info!(peer_id = hex::encode(handshake.peer_id()), "recv handshake");

        Ok((
            Session {
                addr,
                conn,
                tx,
                rx,
                cmd_tx,
                torrent: shared,
                state: State {
                    chocked: true,
                    is_choking: true,
                    interested: false,
                    is_interested: false,
                },
            },
            sender,
        ))
    }

    pub async fn con(&mut self) -> Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match TcpStream::connect(&self.addr).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    if backoff > 64 {
                        return Err(e.into());
                    }
                }
            };

            // Pause execution until the backoff period elapses.
            time::sleep(Duration::from_secs(backoff)).await;

            // Double the backoff.
            backoff *= 2;
        }
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        self.conn.read_frame().await
    }

    pub async fn run(&mut self) -> Result<()> {
        let Some(Frame::Bitfield(_)) = self.recv().await.unwrap() else {
            bail!("expected bitfield frame")
        };

        self.send(&Frame::Interested).await?;
        self.state.interested = true;

        if let Some(Frame::Unchoke) = self.recv().await.unwrap() {
            self.state.is_choking = false;
        } else {
            bail!("expected unchoke frame")
        };

        if self.state.is_choking {
            debug!("cannot make requests while chocked");
            return Ok(());
        }

        if !self.state.interested {
            debug!("cannot make requests while not interested");
            return Ok(());
        }

        loop {
            let Some((index, size)) = self.torrent.piece_picker.lock().unwrap().pick() else {
                info!(?self.addr, "no more pieces to download, closing connection");
                break;
            };

            trace!(?index, ?size, "got next piece");

            let mut download = PieceDownload {
                index,
                data: BytesMut::with_capacity(size),
            };

            let chunk_count = chunk_count(size);
            for i in 0..chunk_count {
                // Check if last chunk.
                let chunk_size = if i == chunk_count - 1 {
                    let rest = size % CHUNK_MAX;
                    if rest == 0 {
                        CHUNK_MAX
                    } else {
                        rest
                    }
                } else {
                    CHUNK_MAX
                };

                let chunk = self.request(index, i * CHUNK_MAX, chunk_size).await?;
                trace!(?chunk.piece_index, chunk_index=i, ?chunk.offset, ?chunk_count, "received piece chunk");

                // TODO handle this errors nicely by holding the state
                // of each block of the piece, so that it can be resumed.
                ensure!(chunk.piece_index as usize == index);
                ensure!(chunk.offset as usize == i * CHUNK_MAX);
                ensure!(chunk.data.len() == chunk_size);

                download.data.extend(&chunk.data);
            }

            ensure!(download.data.len() == size);
            info!(?download.index, "completed");

            let _ = self
                .cmd_tx
                .send(torrent::Command::PieceCompletion(download));

            // Batch some requests to avoid `send request / read response` cycles.
            /* let mut outgoing_requests = 0;
            let max_request_queue_len = 5;

            let pending_chunks: Vec<Chunk> = Vec::new();

            if outgoing_requests >= max_request_queue_len {

            } */

            // NOTE a big assumption is made that all peers have all pieces
            // this is most certainly not the case and we first have to check
            // wich pieces we have and which ones the peer has with the bitfield message.

            // We should also immplement tracking of blocks within each piece
            // to allow for concurrent downloads of a single piece from multiple
            // peers.

            // trace!(?target_request_queue_len, "sending request batch");
            // for _ in 0..target_request_queue_len {}
        }

        Ok(())
    }

    pub async fn request(&mut self, index: usize, begin: usize, length: usize) -> Result<Chunk> {
        let request = Request {
            index: index as u32,
            begin: begin as u32,
            length: length as u32,
        };
        trace!(sending = ?request);
        self.send(&Frame::Request(request)).await?;

        match self.recv().await? {
            Some(Frame::Piece(chunk)) => Ok(chunk),
            other => bail!("expected piece frame got {other:?}"),
        }
    }
}

#[allow(dead_code)]
pub struct Peer {
    pub addr: SocketAddrV4,
    /// The framed connection.
    pub conn: Connection,
    /// The [`Torrent`] shared state with the peers.
    pub torrent: Arc<torrent::Shared>,
    /// The [`Torrent`] transmit cannel.
    pub cmd_tx: torrent::Sender,
    /// The peers state.
    pub state: State,
}

// Challenge...
impl Peer {
    /// Connect to a peer and try to perform a handshake to establish the connection.
    #[instrument(level = "trace", skip(shared, cmd_tx))]
    pub async fn connect(
        addr: SocketAddrV4,
        shared: Arc<torrent::Shared>,
        cmd_tx: torrent::Sender,
    ) -> Result<Self> {
        // TODO: add timeouts.
        trace!("connecting");
        let stream = TcpStream::connect(addr).await?;
        let mut conn = Connection::new(stream);
        trace!("connected");

        trace!("send handshake");
        let handshake = conn.handshake(shared.info_hash).await?;
        info!(peer_id = hex::encode(handshake.peer_id()), "recv handshake");

        Ok(Peer {
            addr,
            conn,
            cmd_tx,
            torrent: shared,
            state: State {
                chocked: true,
                is_choking: true,
                interested: false,
                is_interested: false,
            },
        })
    }

    pub async fn con(&mut self) -> Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match TcpStream::connect(&self.addr).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    if backoff > 64 {
                        return Err(e.into());
                    }
                }
            };

            // Pause execution until the backoff period elapses.
            time::sleep(Duration::from_secs(backoff)).await;

            // Double the backoff.
            backoff *= 2;
        }
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        self.conn.write_frame(frame).await
    }

    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        self.conn.read_frame().await
    }

    pub async fn request(&mut self, index: usize, begin: usize, length: usize) -> Result<Chunk> {
        let request = Request {
            index: index as u32,
            begin: begin as u32,
            length: length as u32,
        };
        trace!(sending = ?request);
        self.send(&Frame::Request(request)).await?;

        match self.recv().await? {
            Some(Frame::Piece(chunk)) => Ok(chunk),
            other => bail!("expected piece frame got {other:?}"),
        }
    }
}
