use std::net::SocketAddrV4;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::{Metainfo, CLIENT_ID};

#[tracing::instrument(level = "trace", skip(meta))]
pub async fn discover(meta: &Metainfo) -> Result<Vec<SocketAddrV4>> {
    let query = Query::new(String::from_utf8_lossy(CLIENT_ID), meta.len());
    let url = format!(
        "{}?info_hash={}&{}",
        meta.announce,
        meta.info.urlencode()?,
        serde_urlencoded::to_string(&query).context("encode tracker query")?
    );
    trace!(GET = url);
    let res = reqwest::get(&url).await.context("get tracker info")?;
    trace!(status = ?res.status());
    let tracker_info = serde_bencode::from_bytes::<Response>(&res.bytes().await?)
        .context("decode tracker response")?;

    // We will only use the `peers` field for this challenge, ignore the `interval` field for now.
    Ok(tracker_info.peers)
}

/// Note: the info hash field is _not_ included.
#[derive(Debug, Serialize)]
pub struct Query {
    /// A unique identifier for your client.
    ///
    /// A string of length 20, each downloader generates its own id at random at
    /// the start of a new download. This value will also almost certainly have to be escaped.
    pub peer_id: String,

    /// The port number this peer is listening on. Common behavior is for a
    /// downloader to try to listen on port 6881 and if that port is taken
    /// try 6882, then 6883, etc. and give up after 6889.
    pub port: u16,

    /// The total amount uploaded so far.
    pub uploaded: usize,

    /// The total amount downloaded so far
    pub downloaded: usize,

    /// The number of bytes left to download.
    ///
    /// Note that this can't be computed from downloaded and the file length
    /// since it might be a resume, and there's a chance that some of the
    /// downloaded data failed an integrity check and had to be re-downloaded.
    pub left: usize,

    /// Whether the peer list should use the compact representation
    ///
    /// The compact representation is more commonly used in the wild, the non-compact
    /// representation is mostly supported for backward-compatibility.
    pub compact: u8,
}

impl Query {
    pub fn new(peer_id: impl Into<String>, left: usize) -> Self {
        Query {
            peer_id: peer_id.into(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left,
            compact: 1,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Response {
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
