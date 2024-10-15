#[repr(u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Kind {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
}

#[derive(Debug)]
pub struct Frame {
    len: u32,
    /// All non-keepalive messages start with a single byte which gives their type.
    /// 'choke', 'unchoke', 'interested', and 'not interested' have no payload.
    // TODO: Express payload type depending on the Kind (might use type state pattern?)
    kind: Kind,

    payload: Option<Box<[u8]>>,
}

impl Frame {
    pub fn new(len: u32, kind: Kind, payload: Option<Box<[u8]>>) -> Self {
        Frame { len, kind, payload }
    }

    pub fn with(kind: Kind, payload: Option<Box<[u8]>>) -> Self {
        Frame {
            len: 1 + payload.as_ref().map_or(0, |p| p.len()) as u32,
            kind,
            payload,
        }
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }
}

impl From<u8> for Kind {
    fn from(value: u8) -> Self {
        use Kind::*;
        match value {
            0 => Choke,
            1 => Unchoke,
            2 => Interested,
            3 => NotInterested,
            4 => Have,
            5 => Bitfield,
            6 => Request,
            7 => Piece,
            8 => Cancel,
            n => unreachable!("kind: {n}"),
        }
    }
}
