use tokio_util::bytes::{BufMut, BytesMut};
use tracing::{debug, trace};

#[repr(C)]
#[derive(Debug)]
pub struct PiecePacket<T: ?Sized = [u8]> {
    // The zero-based piece index.
    pub index: [u8; 4],

    /// The zero-based byte offset within the piece.
    pub begin: [u8; 4],

    /// The data for the piece, usually 2^14 bytes long.
    chunk: T,
}

#[allow(dead_code)]
impl PiecePacket {
    const CHUNK_OFFSET: usize = std::mem::size_of::<PiecePacket<()>>();

    #[tracing::instrument]
    pub fn as_ref_from_bytes(bytes: &[u8]) -> Option<&Self> {
        if bytes.len() < Self::CHUNK_OFFSET {
            return None;
        }
        let len = bytes.len() - Self::CHUNK_OFFSET;
        debug!(lead = Self::CHUNK_OFFSET, ?len);
        trace!("level");
        unsafe { (std::ptr::slice_from_raw_parts(bytes.as_ptr(), len) as *const Self).as_ref() }
    }

    pub fn chunk(&self) -> &[u8] {
        &self.chunk
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut payload = BytesMut::new();
    payload.put_u32(10);
    payload.put_u32(20);
    payload.extend([100, 200]);
    debug!(payload_len = payload.len());

    let piece = PiecePacket::as_ref_from_bytes(&payload[..]);
    debug!(?piece);

    Ok(())
}
