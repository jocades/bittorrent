mod connection;
pub use connection::{Connection, PiecePacket, RequestPacket};

mod handshake;
pub use handshake::HandshakePacket;

pub mod frame;
pub use frame::Frame;

/// Hardcoded peer id for the challenge
pub const PEER_ID: &[u8; 20] = b"jordi123456789abcdef";

pub unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const u8, std::mem::size_of::<T>())
}

pub unsafe fn as_u8_slice_mut<T: Sized>(p: &mut T) -> &mut [u8] {
    std::slice::from_raw_parts_mut((p as *mut T) as *mut u8, std::mem::size_of::<T>())
}
