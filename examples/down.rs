#![allow(dead_code)]
use anyhow::{bail, Result};
use bytes::{Bytes, BytesMut};

#[derive(Debug, PartialEq)]
enum Frame {
    Piece(Chunk),
    Cancel(Chunk),
}

#[derive(Debug, PartialEq)]
struct Chunk {
    // The zero-based piece index.
    index: u32,
    /// The zero-based byte offset within the piece.
    begin: u32,
    /// The data for the piece, usually 2^14 bytes long.
    data: Bytes,
}

#[tokio::main]
async fn main() -> Result<()> {
    let data = BytesMut::from("hello".as_bytes());

    let frame = Frame::Piece(Chunk {
        index: 0,
        begin: 0,
        data: data.into(),
    });

    let Frame::Piece(chunk) = frame else {
        bail!("expected piece frame")
    };

    do_something(&chunk).await;

    Ok(())
}

async fn do_something(piece: &Chunk) {
    println!("{:?}", piece.data);
}
