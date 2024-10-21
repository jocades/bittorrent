use std::{net::SocketAddrV4, sync::Arc};

use anyhow::{bail, ensure, Result};
use tokio::{
    net::TcpStream,
    sync::mpsc,
    task,
    time::{self, Duration},
};
use tokio_util::bytes::BytesMut;
use tracing::{debug, error, info, trace, Instrument};

use super::{Connection, Frame, Request};
use crate::{
    download::{chunk_count, PieceDownload, CHUNK_MAX},
    peer::connection,
    PeerId,
};
use crate::{torrent, Chunk, PieceIndex};

pub type Sender = mpsc::UnboundedSender<Command>;

type Receiver = mpsc::UnboundedReceiver<Command>;

/// A peer session managed by `Torrent` which owns the tasks `JoinHandle`.
pub struct Join {
    /// The tansmit channel to communicate with the peers [`Session`].
    pub tx: Sender,
    /// The peer id received after the handshake.
    pub id: Option<PeerId>,
    /// Information about a peers session.
    pub state: State,
    /// The session task handle. Errors reported at this level should **only**
    /// appear during shutdown, meaning the session did not terminate in a
    /// clean state, all other errors are handled at the session level.
    pub handle: task::JoinHandle<Result<()>>,
}

pub fn spawn(
    addr: SocketAddrV4,
    shared: Arc<torrent::Shared>,
    torrent_tx: torrent::Sender,
) -> Join {
    let (tx, rx) = mpsc::unbounded_channel();
    let sender = tx.clone();

    let mut session = Session {
        addr,
        tx,
        rx,
        torrent: shared,
        torrent_tx,
        state: State::default(),
    };

    let handle = task::spawn(
        async move {
            if let Err(e) = session.run().await {
                error!(cause = %e, "session error");
            }
            Ok(())
        }
        .instrument(tracing::trace_span!("session", %addr)),
    );

    Join {
        tx: sender,
        id: None,
        state: State::default(),
        handle,
    }
}

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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct State {
    /// The state of the connection, only mutable by the peer that holds it.
    pub conn: connection::State,
    /// Whether or not the remote peer has choked this client. When a peer chokes
    /// the client, it is a notification that no requests will be answered until
    /// the client is unchoked. The client should not attempt to send requests
    /// for blocks, and it should consider all pending (unanswered) requests to
    /// be discarded by the remote peer.
    pub is_choked: bool,
    /// Whether or not the remote peer is interested in something this client
    /// has to offer. This is a notification that the remote peer will begin
    /// requesting blocks when the client unchokes them.
    pub is_interested: bool,
    /// If peer is choked, this client will not allow any requests from the peer.
    pub is_peer_choked: bool,
    /// Wether the peer is interested in a piece we have.
    pub is_peer_interested: bool,
}

impl Default for State {
    /// Both sides of the connection start off as choked and not interested.
    fn default() -> Self {
        State {
            conn: connection::State::default(),
            is_choked: true,
            is_interested: false,
            is_peer_choked: true,
            is_peer_interested: false,
        }
    }
}

/// The most essential information of a peer session that is sent to torrent
/// with each session tick.
#[derive(Debug)]
pub(crate) struct Stats {
    /// A snapshot of the session state.
    pub state: State,
    /* /// Various transfer statistics.
    pub counters: ,
    /// The number of pieces the peer has available.
    pub piece_count: usize, */
}

#[allow(dead_code)]
pub(crate) struct Session {
    pub addr: SocketAddrV4,

    pub tx: Sender,
    pub rx: Receiver,

    /// The [`Torrent`] shared state with the peers.
    pub torrent: Arc<torrent::Shared>,
    /// The [`Torrent`] transmit cannel.
    pub torrent_tx: torrent::Sender,
    /// The peers state.
    pub state: State,
}

impl Session {
    pub async fn run(&mut self) -> Result<()> {
        if let Err(e) = self.outbound().await {
            self.state.conn = connection::State::Disconnected;
            self.notify();
            error!(cause = %e, "outbound connection error");
        }
        Ok(())
    }

    /// Attempt to connect to a peer.
    async fn connect(&mut self) -> Result<Connection> {
        let mut backoff = 1;

        loop {
            match TcpStream::connect(&self.addr).await {
                Ok(stream) => return Ok(Connection::new(stream)),
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

    async fn outbound(&mut self) -> Result<()> {
        trace!("connecting");
        self.state.conn = connection::State::Connecting;
        let mut conn = match self.connect().await {
            Ok(conn) => conn,
            Err(e) => bail!("failed connecting to peer {}: {e}", self.addr),
        };
        trace!("connected");

        trace!("send handshake");
        self.state.conn = connection::State::Handshaking;
        let handshake = conn.handshake(self.torrent.info_hash).await?;
        trace!("recv handshake");
        self.state.conn = connection::State::Connected;

        let _ = self.torrent_tx.send(torrent::Command::PeerConnected {
            addr: self.addr,
            id: handshake.take_peer_id(),
        });

        if let Err(e) = self.download(&mut conn).await {
            bail!("error during piece download: {e}")
        }

        Ok(())
    }

    async fn execute(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Shutdown => {
                trace!("shutting down");
            }
            _ => unimplemented!(),
        };
        Ok(())
    }

    async fn download(&mut self, conn: &mut Connection) -> Result<()> {
        let Some(Frame::Bitfield(_)) = conn.read().await? else {
            bail!("expected bitfield frame")
        };

        conn.write(&Frame::Interested).await?;
        self.state.is_interested = true;

        if let Some(Frame::Unchoke) = conn.read().await? {
            self.state.is_peer_choked = false;
        } else {
            bail!("expected unchoke frame")
        }

        if self.state.is_peer_choked {
            debug!("cannot make requests while chocked");
            return Ok(());
        }

        if !self.state.is_interested {
            debug!("cannot make requests while not interested");
            return Ok(());
        }

        loop {
            let Some(index) = self.torrent.piece_picker.lock().unwrap().pick() else {
                info!(?self.addr, "no more pieces to download, closing connection");
                break;
            };
            let size = self.torrent.storage.piece_size(index);
            trace!(%index, %size, "claimed piece");

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

                let req = Request {
                    index: index as u32,
                    begin: (i * CHUNK_MAX) as u32,
                    length: chunk_size as u32,
                };
                trace!(sending = ?req);
                conn.write(&Frame::Request(req)).await?;

                let chunk = match conn.read().await? {
                    Some(Frame::Chunk(chunk)) => chunk,
                    other => bail!("expected piece frame got {other:?}"),
                };

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
                .torrent_tx
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

    /// Returns a summary of the most important information of the session
    /// state to send to torrent.
    fn session_stats(&self) -> Stats {
        Stats {
            state: self.state,
            // counters: self.ctx.counters,
            // piece_count: self.peer.piece_count,
        }
    }

    fn notify(&self) {
        let _ = self.torrent_tx.send(torrent::Command::PeerState {
            addr: self.addr,
            info: self.session_stats(),
        });
    }
}
