use std::{
    collections::HashMap,
    fs::File,
    future::Future,
    io::{prelude::*, SeekFrom, Write},
    net::SocketAddrV4,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::{
    select,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::{self, Duration, Instant},
};

use anyhow::{ensure, Result};
use sha1::{Digest, Sha1};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::{
    peer::{self, connection, session},
    tracker, Metainfo, PeerId, PieceDownload, PiecePicker, Sha1Hash, Storage,
};

pub async fn run(path: impl AsRef<Path>, shutdown: impl Future) {
    let Ok(meta) = Metainfo::read(path.as_ref()) else {
        error!("failed reading metainfo file");
        return;
    };

    let token = CancellationToken::new();
    let mut torrent = Torrent::new(meta, Conf::default(), token.clone());

    select! {
        res = torrent.run() => {
            // If an error is received here, the torrent has failed multiple
            // times and is giving up and shutting down.
            //
            // Errors encountered when running the torrent do not bubble up to
            // this point.
            if let Err(e) = res {
                error!(cause = %e, "failed to run");
            }
        }
        _ = shutdown => {
            // The shutdown signal passed by the caller has been received.
            // Notify torrent and all current tasks.
            info!("shutting down");
            token.cancel()
        }
    }
}

/// The channel for communicating with `Torrent`.
pub(crate) type Sender = UnboundedSender<Command>;

/// The type of channel on which `Torrent` can listen for block write
/// completions.
pub(crate) type Receiver = UnboundedReceiver<Command>;

/// Messages that the torrent can receive from other parts of the engine.
pub(crate) enum Command {
    /// Purely for testing.
    #[allow(dead_code)]
    Ping(Option<oneshot::Sender<String>>),

    /// Sent when some blocks were written to disk or an error ocurred while
    /// writing.
    /// TODO Have a separate task in charge of handling disk writes.
    PieceCompletion(PieceDownload),

    /// A message sent only once, after the peer has been connected.
    PeerConnected { addr: SocketAddrV4, id: PeerId },

    /// Peer sessions periodically send this message when they have a state
    /// change.
    PeerState {
        addr: SocketAddrV4,
        info: session::Stats,
    },

    /// Gracefully shut down the torrent.
    ///
    /// This command tells all active peer sessions of torrent to do the same,
    /// waits for them and announces to trackers our exit.
    #[allow(dead_code)]
    Shutdown,
}

/// The `Torrent` configuration.
pub struct Conf {
    /// The max number of connected peers the torrent should have.
    pub max_connections: usize,
    /// The download destination.
    pub dest: PathBuf,
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            // This value is mostly picked for performance while keeping in mind
            // not to overwhelm the host.
            max_connections: 50,
            dest: Path::new("test_download").into(),
        }
    }
}

/// Information and methods shared with peer sessions in the torrent.
pub(crate) struct Shared {
    /// The torrents general storage information.
    pub storage: Storage,
    /// The piece picker picks the next most optimal piece to download and is
    /// shared by all peers in a torrent
    pub piece_picker: Mutex<PiecePicker>,
    /// The info hash of the torrent, derived from its metainfo. This is used to
    /// identify the torrent with other peers and trackers.
    pub info_hash: Sha1Hash,
}

/// The orchestrator of a torrent download or upload.
pub struct Torrent {
    /// The configuration of this torrent.
    conf: Conf,
    /// Information that is shared with peer sessions.
    shared: Arc<Shared>,
    /// The peers currently in this torrent and their respective task handle.
    peers: HashMap<SocketAddrV4, session::Join>,
    /// The peers returned by the tracker which we can connect.
    available_peers: Vec<SocketAddrV4>,
    /// The holder of this torrents last transmit channel.
    tx: Sender,
    /// The receive channel on which other parts of the system can
    /// communicate with this torrent.
    rx: Receiver,
    /// The trackers we can announce to. For now just their urls.
    #[allow(dead_code)]
    trackers: Vec<Box<str>>,
    /// The time the torrent was first started.
    start_time: Option<Instant>,
    /// The total time the torrent has been running.
    run_duration: Duration,
    /// TODO: Remove this, stays here until fixed `Tracker`
    meta: Metainfo,
    /// The signal to shutdown, given by the caller.
    token: CancellationToken,

    file: File,
}

impl Torrent {
    /// Creates a new `Torrent` instance for downloading or seeding a torrent.
    pub fn new(meta: Metainfo, conf: Conf, shutdown: CancellationToken) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let storage = Storage::new(&meta, &conf.dest);
        let piece_picker = PiecePicker::new(storage.piece_count);
        let trackers = vec![meta.announce.as_str().into()];

        let download = File::create(&storage.dest).unwrap();
        Torrent {
            conf,
            shared: Arc::new(Shared {
                storage,
                piece_picker: Mutex::new(piece_picker),
                info_hash: meta.info.hash().unwrap(),
            }),
            peers: HashMap::new(),
            available_peers: Vec::new(),
            tx,
            rx,
            trackers,
            start_time: None,
            run_duration: Duration::default(),
            meta,
            token: shutdown,
            file: download,
        }
    }

    /// Called by `run` to setup some inner machinery.
    async fn start(&mut self) {
        self.start_time = Some(Instant::now());

        // TODO Spawn tracker peer discovery as a `task` so we dont block torrent.
        // It should also be periodically announcing.
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
                Some(cmd) = self.rx.recv() => self.execute(cmd).await?,
                _ = self.token.cancelled() => {
                    self.shutdown().await;
                    break;
                }
            }
        }
        Ok(())
    }

    /// The torrent tick, as in "the tick of a clock", which runs every second.
    ///
    /// This is **when** the peer discovery and connection is done if necessary
    /// and when stats are updated and reported to the consumers.
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
            let torrent_tx = self.tx.clone();

            let session = session::spawn(addr, shared, torrent_tx);
            self.peers.insert(addr, session);
        }

        info!(
            duration = self.run_duration.as_secs(),
            peers = self.peers.len(),
            available = self.available_peers.len(),
            "STATS"
        );

        Ok(())
    }

    /// Executes a command sent from another part of the system.
    async fn execute(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Ping(resp_tx) => {
                if let Some(resp_tx) = resp_tx {
                    let _ = resp_tx.send("ok".into());
                }
            }
            Command::PeerConnected { addr, id } => {
                if let Some(peer) = self.peers.get_mut(&addr) {
                    debug!(%addr,  id = hex::encode(id), "peer connected with client");
                    peer.id = Some(id);
                }
            }
            Command::PeerState { addr, info } => match self.peers.get_mut(&addr) {
                None => {
                    debug!(%addr, "tried updating non-existent peer");
                }
                Some(peer) => {
                    debug!(%addr, "updating peer state");

                    peer.state = info.state;

                    if peer.state.conn == connection::State::Disconnected {
                        self.peers.remove(&addr);
                        debug!(disconnected = %addr);
                    }

                    debug!(?info.state);
                }
            },
            // These logic should be in another task, maybe use tokios `spawn_blocking`
            // since writing to disk and verifying hashes is expensive and cannot
            // be done asynchronously. Hence this is a big point of contention in `Torrent`.
            // The `Disk` task should handle batch writes every N chunks received.
            Command::PieceCompletion(download) => {
                debug!("piece completion");
                // let mut file = File::create(&self.shared.storage.dest).await?;

                let piece_hash = self.meta.pieces()[download.index];
                trace!("veryfying hash");
                if hex::encode(Sha1::digest(&download.data)) != hex::encode(piece_hash) {
                    // Put piece back in queue, and keep a record of peers who sent us
                    // corrupted pieces to block them in the future.
                    {
                        self.shared
                            .piece_picker
                            .lock()
                            .unwrap()
                            .mark_missing(download.index);
                    }
                    warn!("invalid piece");
                    return Ok(());
                }

                self.file.seek(SeekFrom::Start(
                    (download.index * self.meta.piece_len()) as u64,
                ))?;

                self.file.write_all(&download.data)?;
                debug!(written = ?download.index);

                // Mainly for the callenge since it expects the process to exit.
                let done: bool;
                {
                    let mut guard = self.shared.piece_picker.lock().unwrap();
                    done = guard.mark_complete(download.index);
                }
                info!(?done);
                if done {
                    let size = self.file.metadata().unwrap().size();
                    ensure!(self.meta.len() == size as usize);
                    self.token.cancel()
                }
            }
            _ => unimplemented!(),
        };

        Ok(())
    }

    /// Attempts to perform a clean shutdown of all running tasks.
    async fn shutdown(&mut self) {
        if self.peers.is_empty() {
            info!("no sessions to shutdown");
        } else {
            for (addr, session) in self.peers.drain() {
                info!(%addr, "shutting down session");

                // should handle error and return a ShutdownError saying we could
                // not notify, which could en up with unclean state.
                let _ = session.tx.send(peer::Command::Shutdown);

                // Wait for all sessions to terminate their shutdown process.
                match session.handle.await {
                    Ok(r) => match r {
                        Ok(_) => info!(%addr, "gracefull shutdown"),
                        Err(e) => error!(%addr, "failed shutdown: {e}"),
                    },
                    Err(e) => error!(%addr, "session handle error: {e}"),
                }
            }
        }

        info!("shutdown complete");
        info!(duration = self.run_duration.as_secs(), "STATS");
    }
}
