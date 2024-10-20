use crate::{PeerId, Sha1Hash, PROTOCOL};

unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const u8, std::mem::size_of::<T>())
}

unsafe fn as_u8_slice_mut<T: Sized>(p: &mut T) -> &mut [u8] {
    std::slice::from_raw_parts_mut((p as *mut T) as *mut u8, std::mem::size_of::<T>())
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct HandshakePacket {
    pstrlen: u8,
    pstr: [u8; 19],
    reserved: [u8; 8],
    info_hash: Sha1Hash,
    peer_id: PeerId,
}

impl HandshakePacket {
    pub fn new(info_hash: Sha1Hash, peer_id: PeerId) -> Self {
        Self {
            pstrlen: 19,
            pstr: *PROTOCOL,
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }

    pub const fn len() -> usize {
        std::mem::size_of::<Self>()
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY:
        // - Struct is `repr(C, packed)` and all fields are u8 or fixed-size arrays of u8,
        // ensuring a consistent memory layout valid for any bit pattern.
        // - The lifetime of the returned slice is tied to `&self`, ensuring it's valid.
        // - The size is exactly the size of the struct, so we're not over-reading.
        unsafe { as_u8_slice(self) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { as_u8_slice_mut(self) }
    }

    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != Self::len() {
            return None;
        }
        let mut packet = Self::new([0; 20], [0; 20]);
        // SAFETY:
        // - We have checked that `bytes.len()` equals `size_of::<Self>()`, ensuring we are not over-reading.
        // - We are copying into a properly aligned and sized instance of `Self`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut packet as *mut Self as *mut u8,
                Self::len(),
            );
        }
        Some(packet)
    }

    /// Takes the peer_id consuming `self` so it be sent after a succesfull
    /// handshake without copying.
    pub fn take_peer_id(self) -> [u8; 20] {
        self.peer_id
    }

    pub fn peer_id(&self) -> &[u8; 20] {
        &self.peer_id
    }

    #[allow(dead_code)]
    pub fn info_hash(&self) -> &[u8; 20] {
        &self.info_hash
    }

    #[allow(dead_code)]
    pub fn is_valid_protocol(&self) -> bool {
        self.pstrlen == 19 && self.pstr == *PROTOCOL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Metainfo;

    #[test]
    fn handshake_packet_as_bytes() {
        let torrent = Metainfo::read("./sample.torrent").unwrap();
        let packet = HandshakePacket::new(torrent.info.hash().unwrap(), *crate::CLIENT_ID);
        let bytes = packet.as_bytes();
        assert_eq!(
            bytes,
            &[
                19, 66, 105, 116, 84, 111, 114, 114, 101, 110, 116, 32, 112, 114, 111, 116, 111,
                99, 111, 108, 0, 0, 0, 0, 0, 0, 0, 0, 214, 159, 145, 230, 178, 174, 76, 84, 36,
                104, 209, 7, 58, 113, 212, 234, 19, 135, 154, 127, 106, 111, 114, 100, 105, 49, 50,
                51, 52, 53, 54, 55, 56, 57, 97, 98, 99, 100, 101, 102
            ]
        )
    }

    #[test]
    fn handshake_packet_from_bytes() {
        let bytes = [
            19, 66, 105, 116, 84, 111, 114, 114, 101, 110, 116, 32, 112, 114, 111, 116, 111, 99,
            111, 108, 0, 0, 0, 0, 0, 0, 0, 0, 214, 159, 145, 230, 178, 174, 76, 84, 36, 104, 209,
            7, 58, 113, 212, 234, 19, 135, 154, 127, 106, 111, 114, 100, 105, 49, 50, 51, 52, 53,
            54, 55, 56, 57, 97, 98, 99, 100, 101, 102,
        ];
        let packet = HandshakePacket::from_bytes(&bytes).expect("parse packet from bytes");
        let torrent = Metainfo::read("./sample.torrent").unwrap();
        assert_eq!(packet.info_hash, torrent.info.hash().unwrap());
        assert!(packet.is_valid_protocol());
    }

    #[test]
    fn handshake_packet_from_invalid_bytes() {
        let bytes = [0; 67]; // One byte too short
        assert!(HandshakePacket::from_bytes(&bytes).is_none());
    }
}
