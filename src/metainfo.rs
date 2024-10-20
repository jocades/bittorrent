use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::Sha1Hash;

/// A Metainfo file (also known as .torrent files).
#[derive(Serialize, Deserialize, Debug)]
pub struct Metainfo {
    /// The URL of the tracker.
    pub announce: String,

    pub info: Info,
}

type Pieces = Box<[Sha1Hash]>;

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
    piece_len: usize,

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
    len: usize,

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

    #[serde(skip)]
    hash: Option<Sha1Hash>,
}

impl Info {
    // TODO: apply hash while deserializing
    pub fn hash(&self) -> Result<Sha1Hash> {
        match self.hash {
            Some(hash) => Ok(hash),
            None => {
                let encoded = serde_bencode::to_bytes(&self).context("encode torrent info")?;
                Ok(Sha1::digest(&encoded).into())
            }
        }
    }

    pub fn urlencode(&self) -> Result<String> {
        Ok(self
            .hash()?
            .iter()
            .map(|byte| format!("%{byte:02x}"))
            .collect())
    }
}

impl Metainfo {
    #[inline]
    pub fn pieces(&self) -> &[[u8; 20]] {
        &self.info.pieces
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.info.len
    }

    #[inline]
    pub fn piece_len(&self) -> usize {
        self.info.piece_len
    }

    pub fn from_bytes<B: AsRef<[u8]>>(bytes: B) -> Result<Metainfo> {
        serde_bencode::from_bytes(bytes.as_ref()).context("decode metainfo")
    }

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Metainfo> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
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

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct File {
    /// The length of the file, in bytes.
    pub length: usize,

    /// Subdirectory names for this file, the last of which
    /// is the actual file name (a zero length list is an error case).
    pub path: Vec<String>,
}
