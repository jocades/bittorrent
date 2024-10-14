use std::{net::SocketAddrV4, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// A Metainfo file (also known as .torrent files).
#[derive(Serialize, Deserialize, Debug)]
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
    pub plen: usize,

    /// A string whose length is a multiple of 20. It is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the `piece` at the corresponding index.
    #[serde(with = "pieces")]
    pieces: Pieces,

    /// There is a key `length` or a key `files`, but not both or neither.

    /// If length is present then the download represents a single file, otherwise
    /// it represents a set of files which go in a directory structure.
    ///
    /// In the single file case, length maps to the length of the file in bytes.
    #[serde(rename = "length")]
    pub len: usize,

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

type Pieces = Box<[[u8; 20]]>;

impl Info {
    pub fn hash(&self) -> crate::Result<[u8; 20]> {
        let encoded = serde_bencode::to_bytes(&self).context("encode torrent info")?;
        Ok(Sha1::digest(&encoded).into())
    }

    pub fn urlencode(&self) -> crate::Result<String> {
        Ok(self
            .hash()?
            .iter()
            .map(|byte| format!("%{byte:02x}"))
            .collect())
    }
}

impl Torrent {
    pub fn pieces(&self) -> &[[u8; 20]] {
        &self.info.pieces
    }

    pub fn from_bytes<B: AsRef<[u8]>>(bytes: B) -> crate::Result<Torrent> {
        serde_bencode::from_bytes(bytes.as_ref()).context("parse torrent")
    }

    pub fn read<P: AsRef<Path>>(path: P) -> crate::Result<Torrent> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    pub fn url(&self, query: &TrackerQuery) -> crate::Result<String> {
        Ok(format!(
            "{}?info_hash={}&{}",
            self.announce,
            self.info.urlencode()?,
            serde_urlencoded::to_string(&query).context("parse tracker query")?
        ))
    }
}

mod pieces {
    use super::Pieces;
    use serde::{
        de::{Error, Visitor},
        Deserializer, Serializer,
    };

    pub fn serialize<S>(pieces: &Pieces, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let v = pieces.concat();
        serializer.serialize_bytes(&v)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Pieces, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PiecesVisitor;

        impl<'de> Visitor<'de> for PiecesVisitor {
            type Value = Pieces;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("byte string whose length must be divisible by 20")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v.len() % 20 != 0 {
                    return Err(E::custom(format!("length: {}", v.len())));
                }

                Ok(v.chunks_exact(20)
                    .map(|chunk| chunk.try_into().expect("guaranteed to be length 20"))
                    .collect())
            }
        }

        deserializer.deserialize_bytes(PiecesVisitor)
    }
}

/// Note: the info hash field is _not_ included.
#[derive(Debug, Serialize)]
pub struct TrackerQuery {
    /// A unique identifier for your client.
    ///
    /// A string of length 20 that you get to pick.
    pub peer_id: String,

    /// The port your client is listening on.
    pub port: u16,

    /// The total amount uploaded so far.
    pub uploaded: usize,

    /// The total amount downloaded so far
    pub downloaded: usize,

    /// The number of bytes left to download.
    pub left: usize,

    /// Whether the peer list should use the compact representation
    ///
    /// The compact representation is more commonly used in the wild, the non-compact
    /// representation is mostly supported for backward-compatibility.
    pub compact: u8,
}

#[derive(Debug, Deserialize)]
pub struct TrackerResponse {
    /// An integer, indicating how often your client should make a request to the tracker in seconds.
    ///
    /// You can ignore this value for the purposes of this challenge.
    #[allow(dead_code)]
    pub interval: usize,

    /// A string, which contains list of peers that your client can connect to.
    ///
    /// Each peer is represented using 6 bytes. The first 4 bytes are the peer's IP address and the
    /// last 2 bytes are the peer's port number.
    #[serde(with = "peers")]
    pub peers: Peers,
}

type Peers = Vec<SocketAddrV4>;

mod peers {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::Peers;
    use serde::{
        de::{Error, Visitor},
        Deserializer,
    };

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Peers, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PeersVisitor;

        impl<'de> Visitor<'de> for PeersVisitor {
            type Value = Peers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("byte string whose length must be divisible by 6")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v.len() % 6 != 0 {
                    return Err(E::custom(format!("invalid length: {}", v.len())));
                }

                Ok(v.chunks_exact(6)
                    .map(|chunk| {
                        SocketAddrV4::new(
                            Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                            u16::from_be_bytes([chunk[4], chunk[5]]),
                        )
                    })
                    .collect())
            }
        }

        deserializer.deserialize_bytes(PeersVisitor)
    }
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
