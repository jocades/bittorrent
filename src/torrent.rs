#![allow(dead_code)]
use std::{
    collections::HashMap,
    net::SocketAddrV4,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{bail, Result};
use bytes::Bytes;
use tokio::select;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::{self, JoinHandle};
use tokio::time::{self, Duration, Instant};
use tracing::{error, info, warn};

use crate::{tracker, Frame, Metainfo, Peer, PeerId, PieceIndex, Sha1Hash, Tracker};

struct Storage {
    total_size: usize,
    piece_count: usize,
    piece_size: usize,
    last_piece_size: usize,
    /// The download destination directory of the torrent.
    ///
    /// In case of single file downloads, this is the directory where the file
    /// is downloaded, named as the torrent.
    /// In case of archive downloads, this directory is the download directory
    /// joined by the torrent's name.
    dest: PathBuf,
}

impl Storage {
    /// Extracts storage related information from the torrent metainfo.
    pub fn new(meta: &Metainfo, dest: impl AsRef<Path>) -> Self {
        let total_size = meta.len();
        let piece_count = meta.pieces().len();
        let piece_size = meta.piece_len();
        let last_piece_size = total_size - piece_size * (piece_count - 1);

        Storage {
            total_size,
            piece_count,
            piece_size,
            last_piece_size,
            dest: dest.as_ref().into(),
        }
    }
}

#[derive(Clone)]
enum PieceState {
    Missing,
    Downloading(usize),
    Complete,
}

/// Decides which piece is more suitable to download next. At the moment it is
/// just a queue.
struct PiecePicker {
    /// For now just hold a vec and access by piece index, implement better
    /// strategies in the feature to do quick lookups of which pieces we have,
    /// which pieces we are currently intersted in or which we are serving.
    need: Vec<PieceState>,
    /// The torrents general storage information.
    storage: Storage,
}

impl PiecePicker {
    pub fn new(storage: Storage) -> Self {
        PiecePicker {
            need: vec![PieceState::Missing; storage.piece_count],
            storage,
        }
    }

    /// A very rustic picking implementation :) Needs further improvement obv.
    pub fn pick(&mut self) -> Option<PieceIndex> {
        for (index, piece) in self
            .need
            .iter_mut()
            .filter(|p| matches!(p, PieceState::Missing))
            .enumerate()
        {
            *piece = PieceState::Downloading(0);
            return Some(index);
        }

        None
    }
}

/// This is the only chunk size we're dealing with (except for possibly the
/// last chunk).  It is the widely used and accepted 16 KiB.
const CHUNK_MAX: usize = 1 << 14;

/// A chunk is a fixed size part of a piece, which in turn is a fixed size
/// part of a torrent. Downloading torrents happen at this chunk level
/// granularity.
#[derive(Debug, PartialEq)]
pub struct Chunk {
    // The zero-based piece index.
    pub piece_index: PieceIndex,
    /// The zero-based byte offset within the piece.
    pub offset: u32,
    /// The data of the chunk, usually 2^14 bytes long.
    pub data: Bytes,
}

#[derive(Debug)]
struct PieceDownload {
    index: PieceIndex,
    size: usize,
    chunks: Vec<Chunk>,
}

/// Information and methods shared with peer sessions in the torrent.
///
/// This type contains fields that need to be read or updated by peer sessions.
/// Fields expected to be mutated are thus secured for multi-threading access
/// with various synchronization primitives.
pub struct Shared {
    /// The piece picker picks the next most optimal piece to download and is
    /// shared by all peers in a torrent
    pub piece_picker: Mutex<PiecePicker>,
    /// The info hash of the torrent, derived from its metainfo. This is used to
    /// identify the torrent with other peers and trackers.
    pub info_hash: Sha1Hash,
}

/// The channel for communicating with `Torrent`.
pub(crate) type Sender = UnboundedSender<Command>;

/// The type of channel on which `Torrent` can listen for block write
/// completions.
pub(crate) type Receiver = UnboundedReceiver<Command>;

/// The types of messages that the torrent can receive from other parts of the
/// engine.
#[derive(Debug)]
pub(crate) enum Command {
    /// Sent when some blocks were written to disk or an error ocurred while
    /// writing.
    PieceCompletion(anyhow::Result<PieceCompletion>),
    /// There was an error reading a block.
    /* ReadError {
        block_info: BlockInfo,
        error: ReadError,
    }, */
    /// A message sent only once, after the peer has been connected.
    PeerConnected { addr: SocketAddrV4, id: PeerId },
    /// Peer sessions periodically send this message when they have a state
    /// change.
    /* PeerState { addr: SocketAddr, info: SessionTick }, */
    /// Gracefully shut down the torrent.
    ///
    /// This command tells all active peer sessions of torrent to do the same,
    /// waits for them and announces to trackers our exit.
    Shutdown,
}

/// The type returned on completing a piece.
#[derive(Debug)]
pub(crate) struct PieceCompletion {
    /// The index of the piece.
    pub index: PieceIndex,
    /// Whether the piece is valid. If it's not, it's not written to disk.
    pub is_valid: bool,
}

struct PeerSession {
    // tx:
}

/// The `Torrent` configuration.
pub struct Conf {
    /// The max number of connected peers the torrent should have.
    max_connections: usize,
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            // This value is mostly picked for performance while keeping in mind
            // not to overwhelm the host.
            max_connections: 50,
        }
    }
}

/// The orchestrator of a torrent download or upload.
pub struct Torrent {
    /// The configuration of this torrent.
    conf: Conf,
    /// Information that is shared with peer sessions.
    shared: Arc<Shared>,
    /// The peers currently in this torrent.
    peers: HashMap<SocketAddrV4, JoinHandle<Result<()>>>,
    /// The peers returned by the tracker which we can connect.
    available_peers: Vec<SocketAddrV4>,
    /// The port on which other entities in the system send this torrent
    /// messages.
    ///
    /// The channel has to be wrapped in a `stream::Fuse` so that we can
    /// `select!` on it in the torrent event loop.
    cmd_rx: Receiver,
    /// The trackers we can announce to. For now just a their urls.
    trackers: Vec<Box<str>>,
    /// The time the torrent was first started.
    start_time: Option<Instant>,
    /// TODO: Remove this, stays here until fixed `Tracker`
    meta: Metainfo,
}

impl Torrent {
    /// Creates a new `Torrent` instance for downloading or seeding a torrent.
    fn new(meta: Metainfo, conf: Conf) -> (Self, Sender) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let storage = Storage::new(&meta, "test_download");
        let piece_picker = PiecePicker::new(storage);
        let trackers = vec![meta.announce.as_str().into()];

        (
            Self {
                conf,
                shared: Arc::new(Shared {
                    piece_picker: Mutex::new(piece_picker),
                    info_hash: meta.info.hash().unwrap(),
                }),
                peers: HashMap::new(),
                available_peers: Vec::new(),
                cmd_rx,
                trackers,
                start_time: None,
                meta,
            },
            cmd_tx,
        )
    }

    async fn run(&mut self) -> Result<()> {
        self.start_time = Some(Instant::now());
        // TODO Spawn tracker peer discovery as a `task` to not block torrent.
        // TODO Check the number of peers connected and fill the to at least
        // be on top of the min requester_peers.
        match tracker::discover(&self.meta).await {
            Ok(resp) => {
                info!("announced {resp:?}");
                // TODO check which peers we have.
                self.available_peers.extend(resp.peers);
            }
            Err(e) => {
                warn!("error announcing to tracker {e}");
            }
        };

        let mut tick_timer = time::interval(Duration::from_secs(1));
        let mut last_tick_time = None;

        // The `Torrent` loop, trigger at least every second by the tick timer.
        loop {
            select! {
                tick_time = tick_timer.tick() => self.tick(&mut last_tick_time, tick_time).await?,
                Some(cmd) = self.cmd_rx.recv() => self.execute(&cmd).await,
            };
        }
    }

    /// The torrent tick, as in "the tick of a clock", which runs every second
    /// to perform periodic updates.
    ///
    /// This is when we update statistics and report them to the user, when new
    /// peers are connected, and when periodic announces are made.
    async fn tick(&mut self, last_tick_time: &mut Option<Instant>, now: Instant) -> Result<()> {
        let elapsed_since_last_tick = last_tick_time
            .or(self.start_time)
            .map(|t| now.duration_since(t))
            .unwrap_or_default();
        *last_tick_time = Some(now);

        // Attempt to connect to available peers, if any.
        let conn_count = std::cmp::min(
            self.available_peers.len(),
            self.conf.max_connections - self.peers.len(),
        );
        if conn_count == 0 {
            warn!("no peers to connect to");
        }

        for addr in self.available_peers.drain(..conn_count) {
            let shared = Arc::clone(&self.shared);
            info!("connecting to {conn_count} peer(s)");

            let handle = task::spawn(async move {
                let mut peer = match Peer::connect(addr, shared).await {
                    Ok(peer) => peer,
                    Err(why) => {
                        warn!("failed connecting to peer {addr}: {why}");
                        return Ok(());
                    }
                };

                let Some(Frame::Bitfield(_)) = peer.recv().await? else {
                    bail!("expected bitfield frame")
                };

                peer.send(&Frame::Interested).await?;

                Ok(())
            });

            self.peers.insert(addr, handle);
        }

        Ok(())
    }

    /// Executes a command sent from another part of the system.
    async fn execute(&mut self, _cmd: &Command) {
        todo!()
    }
}
