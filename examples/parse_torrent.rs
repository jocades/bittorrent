//! Taken from [https://github.com/toby/serde-bencode/blob/master/examples/parse_torrent.rs]
use serde::Deserialize;
use serde_bencode::de;
use serde_bytes::ByteBuf;
use std::io::{self, Read};

#[derive(Debug, Deserialize)]
struct Node(String, i64);

#[derive(Debug, Deserialize)]
struct File {
    path: Vec<String>,
    length: i64,
    md5sum: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Info {
    pub name: String,
    pub pieces: ByteBuf,
    #[serde(rename = "piece length")]
    pub piece_length: i64,
    pub md5sum: Option<String>,
    pub length: Option<i64>,
    pub files: Option<Vec<File>>,
    pub private: Option<u8>,
    pub path: Option<Vec<String>>,
    #[serde(rename = "root hash")]
    pub root_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Torrent {
    info: Info,
    announce: Option<String>,
    nodes: Option<Vec<Node>>,
    encoding: Option<String>,
    httpseeds: Option<Vec<String>>,
    #[serde(rename = "announce-list")]
    announce_list: Option<Vec<Vec<String>>>,
    #[serde(rename = "creation date")]
    creation_date: Option<i64>,
    #[serde(rename = "comment")]
    comment: Option<String>,
    #[serde(rename = "created by")]
    created_by: Option<String>,
}

fn render_torrent(torrent: &Torrent) {
    println!("name:\t\t{}", torrent.info.name);
    println!("announce:\t{:?}", torrent.announce);
    println!("nodes:\t\t{:?}", torrent.nodes);
    if let Some(al) = &torrent.announce_list {
        for a in al {
            println!("announce list:\t{}", a[0]);
        }
    }
    println!("httpseeds:\t{:?}", torrent.httpseeds);
    println!("creation date:\t{:?}", torrent.creation_date);
    println!("comment:\t{:?}", torrent.comment);
    println!("created by:\t{:?}", torrent.created_by);
    println!("encoding:\t{:?}", torrent.encoding);
    println!("piece length:\t{:?}", torrent.info.piece_length);
    println!("private:\t{:?}", torrent.info.private);
    println!("root hash:\t{:?}", torrent.info.root_hash);
    println!("md5sum:\t\t{:?}", torrent.info.md5sum);
    println!("path:\t\t{:?}", torrent.info.path);
    if let Some(files) = &torrent.info.files {
        for f in files {
            println!("file path:\t{:?}", f.path);
            println!("file length:\t{}", f.length);
            println!("file md5sum:\t{:?}", f.md5sum);
        }
    }
}

fn main() {
    let mut buffer = Vec::new();
    let mut handle = io::stdin().lock();
    match handle.read_to_end(&mut buffer) {
        Ok(_) => match de::from_bytes::<Torrent>(&buffer) {
            Ok(t) => render_torrent(&t),
            Err(e) => println!("ERROR: {e:?}"),
        },
        Err(e) => println!("ERROR: {e:?}"),
    }
}
