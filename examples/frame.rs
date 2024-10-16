#![allow(unused)]
use bytes::{Buf, BufMut, Bytes, BytesMut};

#[repr(u8)]
#[derive(Debug, PartialEq)]
enum Frame {
    Choke = 0,
    Have(u32) = 1,
}

fn main() -> anyhow::Result<()> {
    let bytes = Bytes::from(vec![1, 2, 3]);
    let n = u8::from(Frame::Choke);

    Ok(())
}

impl From<Frame> for u8 {
    fn from(value: Frame) -> Self {
        match value {
            Frame::Choke => 0,
            Frame::Have(_) => 1,
        }
    }
}

/* impl From<&Frame> for u8 {
    fn from(value: &Frame) -> Self {
        // *value as u8
    }
} */
