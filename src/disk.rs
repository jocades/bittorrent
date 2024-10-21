use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{prelude::*, SeekFrom, Write},
    path::{Path, PathBuf},
};

use sha1::Sha1;
use tokio::{
    select,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task,
    time::{self, Duration, Instant},
};

use anyhow::Result;
use tracing::{error, instrument::Instrumented, trace, Instrument};

use crate::{
    download::{Chunk, PieceDownload},
    metainfo::Metainfo,
    peer, torrent, Sha1Hash, Storage,
};

/// The channel for communicating with `Disk`.
pub(crate) type Sender = UnboundedSender<Command>;

/// The channel on which the system communicates with `Disk`.
pub(crate) type Receiver = UnboundedReceiver<Command>;

/// Spawn the [`Disk`] task on a thread where blocking is acceptable.
pub fn spawn() -> (Sender, task::JoinHandle<Instrumented<()>>) {
    let (tx, rx) = mpsc::unbounded_channel();

    let mut disk = Disk {
        rx,
        files: Vec::new(),
        dest: Path::new("test_download_dir").into(),
        queue: VecDeque::new(),
    };

    let handle = task::spawn_blocking(move || {
        {
            if let Err(e) = disk.run() {
                error!(cause = %e, "disk error");
            }
        }
        .instrument(tracing::trace_span!("disk"))
    });

    (tx, handle)
}

/// The type of commands that the disk can execute.
#[derive(Debug)]
pub(crate) enum Command {
    /// Allocate a new torrent in `Disk`.
    NewTorrent {
        // storage_info: StorageInfo,
        piece_hashes: Box<Sha1Hash>,
        torrent_tx: torrent::Sender,
    },
    /// Request to write a block to disk.
    WriteChunk(Chunk),
    /// Request to read a chunk from disk and return it via the sender.
    ReadChunk {
        chunk: Chunk,
        // result_tx: peer::Sender,
    },
    /// Eventually shut down the disk task.
    Shutdown,
}

pub struct Disk {
    rx: Receiver,
    pub files: Vec<File>,
    pub dest: PathBuf,
    queue: VecDeque<Chunk>,
}

const MAX_QUEUE: usize = 10;

impl Disk {
    pub fn allocate(&mut self, piece_count: usize) -> Result<()> {
        trace!(piece_count, "allocating");
        fs::create_dir(&self.dest)?;

        for index in 0..piece_count {
            let file = File::create(self.dest.join(format!("piece_{index}")))?;
            self.files.push(file)
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            let Some(cmd) = self.rx.blocking_recv() else {
                break;
            };

            match cmd {
                Command::WriteChunk(chunk) => {
                    if self.queue.len() > MAX_QUEUE {
                        // write chunks
                    }
                }
                other => unimplemented!("{other:?}"),
            }
        }
        Ok(())
    }
}
