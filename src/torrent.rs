use std::{
    collections::HashMap,
    io::SeekFrom,
    net::SocketAddrV4,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{bail, ensure, Result};
use bytes::BytesMut;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::select;
use tokio::signal;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task;
use tokio::time::{self, Duration, Instant};
use tokio::{fs::File, sync::oneshot};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::download::{chunk_count, CHUNK_MAX};
use crate::{tracker, Frame, Metainfo, Peer, PeerId, PieceDownload, PieceIndex, Sha1Hash};

#[allow(dead_code)]
pub(crate) struct Storage {
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
        let piece_size = meta.piece_len();
        let piece_count = meta.pieces().len();
        let last_piece_size = total_size - piece_size * (piece_count - 1);
        trace!(
            ?total_size,
            ?piece_size,
            ?piece_count,
            ?last_piece_size,
            "storage"
        );

        Storage {
            total_size,
            piece_count,
            piece_size,
            last_piece_size,
            dest: dest.as_ref().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PieceState {
    Missing,
    Downloading,
    Complete,
}

/// Decides which piece is more suitable to download next. At the moment it is
/// just a queue.
pub struct PiecePicker {
    /// For now just hold a vec and access by piece index, implement better
    /// strategies in the feature to do quick lookups of which pieces we have,
    /// which pieces we are currently intersted in or which we are serving.
    need: Vec<PieceState>,
    /// The torrents general storage information.
    pub storage: Storage,
}

impl PiecePicker {
    pub fn new(storage: Storage) -> Self {
        PiecePicker {
            need: vec![PieceState::Missing; storage.piece_count],
            storage,
        }
    }

    /// A very rustic picking implementation :) Needs further improvement obv.
    pub fn pick(&mut self) -> Option<(PieceIndex, usize)> {
        trace!(?self.need, "PICK");
        for (index, piece) in self.need.iter_mut().enumerate() {
            match piece {
                PieceState::Missing => {
                    *piece = PieceState::Downloading;
                    return Some((
                        index,
                        if index == self.storage.piece_count - 1 {
                            self.storage.last_piece_size
                        } else {
                            self.storage.piece_size
                        },
                    ));
                }
                _ => (),
            }
        }

        None
    }

    #[instrument(skip(self))]
    pub fn mark_complete(&mut self, index: PieceIndex) -> bool {
        *&mut self.need[index] = PieceState::Complete;
        for piece in self.need.iter() {
            if *piece != PieceState::Complete {
                return false;
            }
        }
        true
    }
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
    /// Purely for testing.
    #[allow(dead_code)]
    Ping(Option<oneshot::Sender<String>>),
    /// Sent when some blocks were written to disk or an error ocurred while
    /// writing.
    /// TODO Have a separate task in charge of handling disk writes.
    PieceCompletion(PieceDownload),
    // PieceCompletion(anyhow::Result<PieceCompletion>),
    /// There was an error reading a block.
    /* ReadError {
        block_info: BlockInfo,
        error: ReadError,
    }, */
    /// A message sent only once, after the peer has been connected.
    #[allow(dead_code)]
    PeerConnected { addr: SocketAddrV4, id: PeerId },
    /// Peer sessions periodically send this message when they have a state
    /// change.
    /* PeerState { addr: SocketAddr, info: SessionTick }, */
    /// Gracefully shut down the torrent.
    ///
    /// This command tells all active peer sessions of torrent to do the same,
    /// waits for them and announces to trackers our exit.
    #[allow(dead_code)]
    Shutdown,
}

/* /// The type returned on completing a piece.
#[derive(Debug)]
pub(crate) struct PieceCompletion {
    /// The index of the piece.
    pub index: PieceIndex,
    /// Whether the piece is valid. If it's not, it's not written to disk.
    pub is_valid: bool,
} */

/// The `Torrent` configuration.
pub struct Conf {
    /// The max number of connected peers the torrent should have.
    pub max_connections: usize,
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
    /// The peers currently in this torrent with their task handle.
    peers: HashMap<SocketAddrV4, task::JoinHandle<Result<()>>>,
    /// The peers returned by the tracker which we can connect.
    available_peers: Vec<SocketAddrV4>,
    /// The holder of the last channels transmit.
    cmd_tx: Sender,
    /// The channels receive on which other entities in the system send this torrent
    /// messages.
    cmd_rx: Receiver,
    /// The trackers we can announce to. For now just their urls.
    #[allow(dead_code)]
    trackers: Vec<Box<str>>,
    /// The time the torrent was first started.
    start_time: Option<Instant>,
    /// The total time the torrent has been running.
    run_duration: Duration,
    /// TODO: Remove this, stays here until fixed `Tracker`
    meta: Metainfo,
}

impl Torrent {
    /// Creates a new `Torrent` instance for downloading or seeding a torrent.
    pub fn new(meta: Metainfo, conf: Conf) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let storage = Storage::new(&meta, "test_download");
        let piece_picker = PiecePicker::new(storage);
        let trackers = vec![meta.announce.as_str().into()];

        Self {
            conf,
            shared: Arc::new(Shared {
                piece_picker: Mutex::new(piece_picker),
                info_hash: meta.info.hash().unwrap(),
            }),
            peers: HashMap::new(),
            available_peers: Vec::new(),
            cmd_tx,
            cmd_rx,
            trackers,
            start_time: None,
            run_duration: Duration::default(),
            meta,
        }
    }

    /// Called by `run` to setup some inner machinery.
    async fn start(&mut self) {
        info!("torrent started");
        self.start_time = Some(Instant::now());
        // TODO Spawn tracker peer discovery as a `task` so we dont block torrent.
        match tracker::discover(&self.meta).await {
            Ok(resp) => {
                info!("announced {resp:?}");
                self.available_peers.extend(resp.peers);
            }
            Err(e) => {
                warn!("error announcing to tracker {e}");
            }
        };
    }

    /// Run this torrent and enter the network!
    pub async fn run(&mut self) -> Result<()> {
        self.start().await;

        let mut tick_timer = time::interval(Duration::from_secs(1));
        let mut last_tick_time = None;

        // The `Torrent` loop, triggered at least every second by the tick timer.
        loop {
            select! {
                tick_time = tick_timer.tick() => self.tick(&mut last_tick_time, tick_time).await?,
                Some(cmd) = self.cmd_rx.recv() => self.execute(cmd).await?,
                _ = signal::ctrl_c() => {
                    self.shutdown().await;
                    break;
                },
            };
        }
        Ok(())
    }

    async fn shutdown(&mut self) {
        for (addr, handle) in self.peers.drain() {
            match handle.await {
                Ok(result) => match result {
                    Ok(_) => {
                        info!("connection with peer {addr} closed succesfully");
                    }
                    Err(e) => {
                        error!("error closing connection with peer {addr}: {e}");
                    }
                },
                Err(e) => {
                    error!("sutdown handle error: {e}");
                }
            }
        }

        info!(
            "Shutdown complete; run duration: {}",
            self.run_duration.as_secs()
        );
    }

    /// The torrent tick, as in "the tick of a clock", which runs every second
    /// to perform periodic updates.
    ///
    /// This is **when** we update statistics and report them to the user, when new
    /// peers are connected, and when periodic announces are made.
    async fn tick(&mut self, last_tick_time: &mut Option<Instant>, now: Instant) -> Result<()> {
        let elapsed_since_last_tick = last_tick_time
            .or(self.start_time)
            .map(|t| now.duration_since(t))
            .unwrap_or_default();
        self.run_duration += elapsed_since_last_tick;
        *last_tick_time = Some(now);

        // Attempt to connect to available peers, if any.
        let conn_count = std::cmp::min(
            self.available_peers.len(),
            self.conf.max_connections - self.peers.len(),
        );
        if conn_count == 0 {
            warn!("no peers to connect to");
        }

        info!("connecting to {conn_count} peer(s)");
        for addr in self.available_peers.drain(..) {
            let shared = Arc::clone(&self.shared);
            let cmd_tx = self.cmd_tx.clone();

            let handle = task::spawn(async move {
                let mut peer = match Peer::connect(addr, shared, cmd_tx).await {
                    Ok(peer) => peer,
                    Err(why) => {
                        warn!("failed connecting to peer {addr}: {why}");
                        return Ok(());
                    }
                };

                let span = tracing::trace_span!("DOWNLOAD", ?addr);
                let _enter = span.enter();

                let Some(Frame::Bitfield(_)) = peer.recv().await.unwrap() else {
                    bail!("expected bitfield frame")
                };

                peer.send(&Frame::Interested).await?;
                peer.interested = true;

                if let Some(Frame::Unchoke) = peer.recv().await.unwrap() {
                    peer.is_choking = false;
                } else {
                    bail!("expected unchoke frame")
                };

                if peer.is_choking {
                    debug!("cannot make requests while chocked");
                    return Ok(());
                }

                if !peer.interested {
                    debug!("cannot make requests while not interested");
                    return Ok(());
                }

                loop {
                    let Some((index, size)) = peer.shared.piece_picker.lock().unwrap().pick()
                    else {
                        info!(?peer.addr, "no more pieces to download, closing connection");
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

                        let chunk = peer.request(index, i * CHUNK_MAX, chunk_size).await?;
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

                    let _ = peer.cmd_tx.send(Command::PieceCompletion(download));

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
            });

            self.peers.insert(addr, handle);
        }

        debug!(
            "STATS: \
            elapsed {}s",
            /* download: {} b/s (peak: {} b/s, total: {} b) wasted: {} b \
            upload: {} b/s (peak: {} b/s, total: {} b)", */
            self.run_duration.as_secs(),
            /* self.counters.payload.down.avg(),
            self.counters.payload.down.peak(),
            self.counters.payload.down.total(),
            self.counters.waste.total(),
            self.counters.payload.up.avg(),
            self.counters.payload.up.peak(),
            self.counters.payload.up.total(), */
        );

        Ok(())
    }

    /// Executes a command sent from another part of the system.
    async fn execute(&mut self, cmd: Command) -> Result<()> {
        info!("EXECUTE");
        match cmd {
            Command::Ping(resp_tx) => {
                trace!("ping");
                if let Some(resp_tx) = resp_tx {
                    let _ = resp_tx.send("ok".into());
                }
            }
            // These logic should be in another task, maybe use tokios `spawn_blocing`
            // since writing to disk and verifying hashes is expensive and cannot
            // be done asynchronously. Hence these is a big point of contention in `Torrent`.
            // The `Disk` task should handle batch writes every N chunks received.
            Command::PieceCompletion(download) => {
                trace!("piece completion");
                let mut file = File::create("test_download").await?;

                let piece_hash = self.meta.pieces()[download.index];
                trace!("veryfying hash");
                if hex::encode(Sha1::digest(&download.data)) != hex::encode(piece_hash) {
                    // Put piece back in queue, and keep a record of peers who sent us
                    // corrupted pieces to block them in the future.
                    {
                        let mut guard = self.shared.piece_picker.lock().unwrap();
                        let piece = &mut guard.need[download.index];
                        *piece = PieceState::Missing;
                        trace!("invalid piece");
                    };
                    return Ok(());
                }

                file.seek(SeekFrom::Start(
                    (download.index * self.meta.piece_len()) as u64,
                ))
                .await?;
                file.write_all(&download.data).await?;
                info!(?download.index, "PIECE WRITTEN");

                // Mainly for the callenge since it expects the process to exit.
                let done: bool;
                {
                    let mut guard = self.shared.piece_picker.lock().unwrap();
                    done = guard.mark_complete(download.index);
                }
                info!(?done);
                if done {
                    self.shutdown().await
                }
            }
            _ => unimplemented!(),
        };

        Ok(())
    }
}
