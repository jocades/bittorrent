mod connection;
pub use connection::Connection;

mod handshake;
pub use handshake::HandshakePacket;

pub mod frame;
pub use frame::Frame;

/// Hardcoded peer id for the challenge
pub const PEER_ID: &[u8; 20] = b"jordi123456789abcdef";
