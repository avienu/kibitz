//! Minimal FEN placement reader. The verbalizer may only state chess facts
//! present in the record; the record's own FEN is the authority for which
//! piece (and whose) stands on a referenced square. No move generation, no
//! legality checking — just square -> (color, piece kind).

use silman_core::record::SideColor;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    /// Template key for this piece's noun ("piece.knight" -> "knight").
    pub(crate) fn template_key(self) -> &'static str {
        match self {
            PieceKind::Pawn => "piece.pawn",
            PieceKind::Knight => "piece.knight",
            PieceKind::Bishop => "piece.bishop",
            PieceKind::Rook => "piece.rook",
            PieceKind::Queen => "piece.queen",
            PieceKind::King => "piece.king",
        }
    }
}

pub(crate) struct Board {
    squares: BTreeMap<String, (SideColor, PieceKind)>,
}

impl Board {
    /// Parse the placement field of a FEN. Malformed input yields an empty
    /// board; lookups then fall back to neutral piece phrases.
    pub(crate) fn from_fen(fen: &str) -> Self {
        let mut squares = BTreeMap::new();
        let placement = fen.split_whitespace().next().unwrap_or("");
        for (rank_index, rank) in placement.split('/').take(8).enumerate() {
            let rank_number = 8 - rank_index;
            let mut file = 0u32;
            for c in rank.chars() {
                if let Some(step) = c.to_digit(10) {
                    file += step;
                    continue;
                }
                if file >= 8 {
                    break;
                }
                let color = if c.is_ascii_uppercase() {
                    SideColor::White
                } else {
                    SideColor::Black
                };
                let kind = match c.to_ascii_lowercase() {
                    'p' => PieceKind::Pawn,
                    'n' => PieceKind::Knight,
                    'b' => PieceKind::Bishop,
                    'r' => PieceKind::Rook,
                    'q' => PieceKind::Queen,
                    'k' => PieceKind::King,
                    _ => {
                        file += 1;
                        continue;
                    }
                };
                let square = format!("{}{rank_number}", (b'a' + file as u8) as char);
                squares.insert(square, (color, kind));
                file += 1;
            }
        }
        Board { squares }
    }

    pub(crate) fn piece_at(&self, square: &str) -> Option<(SideColor, PieceKind)> {
        self.squares.get(square).copied()
    }
}
