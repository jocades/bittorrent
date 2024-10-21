use tracing::{instrument, trace};

use crate::PieceIndex;

#[derive(Debug, Clone, PartialEq)]
enum PieceState {
    Missing,
    Downloading,
    Complete,
}

/// Also known as `PieceManger`. Is in charge of handling the pices of a
/// specific torrent, which ones we have and in which state they are. In the
/// future implement logic to decide which piece is more suitable to download
/// next. At the moment the implementation is very poor.
pub struct PiecePicker {
    /// For now just hold a vec and access by piece index, implement better
    /// strategies in the feature to do quick lookups of which pieces we have,
    /// which pieces we are currently intersted in or which we are serving.
    need: Vec<PieceState>,
}

impl PiecePicker {
    pub fn new(piece_count: usize) -> Self {
        PiecePicker {
            need: vec![PieceState::Missing; piece_count],
        }
    }

    /// A very rustic picking implementation :) Needs further improvement obv.
    pub fn pick(&mut self) -> Option<PieceIndex> {
        trace!(pick = ?self.need);
        for (index, piece) in self.need.iter_mut().enumerate() {
            if *piece == PieceState::Missing {
                *piece = PieceState::Downloading;
                return Some(index);
            }
        }

        None
    }

    /// Marks a piece as complete, returns true if all pieces are downloaded.
    #[instrument(skip(self))]
    pub fn mark_complete(&mut self, index: PieceIndex) -> bool {
        *&mut self.need[index] = PieceState::Complete;
        for piece in self.need.iter() {
            if *piece != PieceState::Complete {
                return false;
            }
        }
        true
    }

    /// Marks a piece as missing.
    #[instrument(skip(self))]
    pub fn mark_missing(&mut self, index: PieceIndex) {
        *&mut self.need[index] = PieceState::Missing
    }
}
