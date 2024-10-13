use anyhow::Context;
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
    pieces: Pieces,

    /// There is a key `length` or a key `files`, but not both or neither.

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

#[derive(Debug)]
pub struct Pieces(Vec<[u8; 20]>);

impl Torrent {
    pub fn pieces(&self) -> &[[u8; 20]] {
        self.info.pieces.0.as_slice()
    }

    pub fn from_bytes<B: AsRef<[u8]>>(bytes: B) -> crate::Result<Torrent> {
        serde_bencode::from_bytes(bytes.as_ref()).context("failed to parse torrent")
    }
}

mod de {
    use super::Pieces;
    use serde::{
        de::{Error, Visitor},
        Deserialize, Deserializer,
    };

    struct PiecesVisitor;

    impl<'de> Visitor<'de> for PiecesVisitor {
        type Value = Pieces;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a byte string whose length must be divisible by 20")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if v.len() % 20 != 0 {
                return Err(E::custom(format!("length: {}", v.len())));
            }

            let pieces = Pieces(
                v.chunks(20)
                    .map(|chunk| chunk.try_into().expect("guaranteed to be length 20"))
                    .collect(),
            );

            Ok(pieces)
        }
    }

    impl<'de> Deserialize<'de> for Pieces {
        fn deserialize<D>(deserializer: D) -> Result<Pieces, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PiecesVisitor)
        }
    }
}

mod ser {
    use super::Pieces;
    use serde::{Serialize, Serializer};

    impl Serialize for Pieces {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let v = self.0.concat();
            serializer.serialize_bytes(&v)
        }
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
