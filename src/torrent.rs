use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// A Metainfo file (also known as .torrent files).
#[derive(Deserialize, Debug)]
pub struct Torrent {
    /// The URL of the tracker.
    pub announce: String,

    pub info: Info,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
    /// The suggested name to save the file (or directory) as. It is purely advisory.
    ///
    /// In the single file case, the name key is the name of a file, in the muliple file case, it's
    /// the name of a directory.
    pub name: String,

    /// The number of bytes in each piece the file is split into.
    ///
    /// For the purposes of transfer, files are split into fixed-size pieces which are all the same
    /// length except for possibly the last one which may be truncated. piece length is almost
    /// always a power of two, most commonly 2^18 = 256K (BitTorrent prior to version 3.2 uses 2
    /// 20 = 1 M as default).
    #[serde(rename = "piece length")]
    pub piece_length: usize,

    /// A string whose length is a multiple of 20. It is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the `piece` at the corresponding index.
    pub pieces: ByteBuf,

    /// There is also a key `length` or a key `files`, but not both or neither.

    /// If length is present then the download represents a single file, otherwise
    /// it represents a set of files which go in a directory structure.
    ///
    /// In the single file case, length maps to the length of the file in bytes.
    pub length: usize,

    /// For the purposes of the other keys, the multi-file case is treated as
    /// only having a single file by concatenating the files in the order they appear in the files list.
    /// The files list is the value files maps to, and is a list of dictionaries.
    /// For this challenge we will only implement support for single-file torrents.
    pub files: Option<Vec<File>>,

    /// Some additional metadata that can be added to the torrent.
    #[serde(rename = "creation date")]
    pub creation_date: Option<usize>,
    #[serde(rename = "comment")]
    pub comment: Option<String>,
    #[serde(rename = "created by")]
    pub created_by: Option<String>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct File {
    /// The length of the file, in bytes.
    pub length: usize,

    /// Subdirectory names for this file, the last of which
    /// is the actual file name (a zero length list is an error case).
    pub path: Vec<String>,
}
