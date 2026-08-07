//! Effective force: material counted where it can actually fight.
//!
//! Material is a board-wide sum, and that is a lie the moment the game
//! has a location. A rook on a8 that needs four moves to reach the
//! kingside contributes nothing to a fight happening at h2 — it is, in
//! the maintainer's phrase, "almost like nothing". Being a pawn or an
//! exchange down is a perfectly good trade for having twice the force
//! where the game is being decided, which is what the corpus asks for
//! under "activity-over-material" and "seize-key-moment".
//!
//! So: split the board into three sectors, and weight every piece by how
//! many moves it needs to reach the one in question. The routing search
//! is the same one the maneuver layer uses, so blockers and safety are
//! already accounted for — a piece that cannot get there safely does not
//! count as force, which is the entire point.

use cozy_chess::{BitBoard, Board, Color, File, Piece, Square};

/// The three theatres. Files, not ranks: attacks happen on a wing or in
/// the centre, and a piece's file distance is what keeps it out of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector {
    Queenside,
    Center,
    Kingside,
}

impl Sector {
    pub fn mask(self) -> BitBoard {
        match self {
            Sector::Queenside => File::A.bitboard() | File::B.bitboard() | File::C.bitboard(),
            Sector::Center => File::D.bitboard() | File::E.bitboard(),
            Sector::Kingside => File::F.bitboard() | File::G.bitboard() | File::H.bitboard(),
        }
    }

    pub fn of(square: Square) -> Sector {
        match square.file() {
            File::A | File::B | File::C => Sector::Queenside,
            File::D | File::E => Sector::Center,
            _ => Sector::Kingside,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Sector::Queenside => "queenside",
            Sector::Center => "center",
            Sector::Kingside => "kingside",
        }
    }
}

/// How much of `color`'s army can fight in `sector`, in centipawns.
///
/// A piece already there counts in full. Otherwise it is discounted by
/// the number of moves it needs to arrive — a piece three moves away is
/// worth a third of itself to a fight happening now, and one that cannot
/// arrive safely at all is worth nothing. Kings are excluded: the king is
/// what the fight is ABOUT, not a unit of attacking force.
pub fn force_in(board: &Board, color: Color, sector: Sector) -> i32 {
    let mask = sector.mask();
    let mut total = 0i32;
    for piece in [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
    ] {
        let value = crate::see::piece_value(piece);
        for from in board.colored_pieces(color, piece) {
            if mask.has(from) {
                total += value;
                continue;
            }
            // Pawns do not redeploy across the board; one that is not
            // already in the sector is not going to join this fight.
            if piece == Piece::Pawn {
                continue;
            }
            let targets = mask & !board.colors(color);
            let Some(route) = crate::route::route_to(board, color, piece, from, targets, &|_| true)
            else {
                continue;
            };
            total += match route.moves() {
                1 => value,
                2 => value * 2 / 3,
                3 => value / 3,
                _ => 0,
            };
        }
    }
    total
}

/// The sector where `color` has the biggest advantage in effective force,
/// with the margin in centipawns — or `None` when it holds no edge worth
/// the name anywhere.
///
/// `min_margin` is deliberately a whole minor piece by default: a small
/// local surplus is normal and says nothing, while "twice the army in
/// this quarter of the board" is a plan.
pub fn strongest_sector(board: &Board, color: Color, min_margin: i32) -> Option<(Sector, i32)> {
    [Sector::Queenside, Sector::Center, Sector::Kingside]
        .into_iter()
        .map(|s| (s, force_in(board, color, s) - force_in(board, !color, s)))
        .filter(|(_, margin)| *margin >= min_margin)
        .max_by_key(|(_, margin)| *margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn board(fen: &str) -> Board {
        Board::from_str(fen).expect("fen")
    }

    /// A rook that cannot reach the fight is not force. Both sides own a
    /// rook, but White's is already on the kingside and Black's is walled
    /// in on a8 behind its own pieces.
    #[test]
    fn distance_discounts_a_piece_out_of_the_theatre() {
        let b = board("rnb1k3/pppp4/8/8/8/8/5PPP/5RK1 w - - 0 1");
        let w = force_in(&b, Color::White, Sector::Kingside);
        let bl = force_in(&b, Color::Black, Sector::Kingside);
        assert!(
            w > bl,
            "white {w} should out-gun black {bl} on the kingside"
        );
    }

    /// The opening position is symmetrical, so no sector holds an edge.
    #[test]
    fn the_start_position_has_no_strong_sector() {
        let b = board("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert!(strongest_sector(&b, Color::White, 300).is_none());
        assert!(strongest_sector(&b, Color::Black, 300).is_none());
    }

    /// Material and force disagree, which is the whole reason this
    /// module exists: Black is down a clear exchange yet owns the
    /// kingside, because White's extra force is on the wrong wing and
    /// cannot get back.
    #[test]
    fn force_and_material_can_point_opposite_ways() {
        let b = board("r5k1/5ppp/8/8/8/6qn/5PPP/R4RK1 w - - 0 1");
        let material_white: i32 = [Piece::Rook, Piece::Rook]
            .iter()
            .map(|p| crate::see::piece_value(*p))
            .sum();
        let material_black =
            crate::see::piece_value(Piece::Queen) + crate::see::piece_value(Piece::Knight);
        assert!(
            material_black > material_white - 200,
            "sanity on the fixture"
        );
        let (sector, margin) =
            strongest_sector(&b, Color::Black, 300).expect("black owns a sector");
        assert_eq!(sector, Sector::Kingside);
        assert!(margin >= 300, "margin {margin}");
    }
}
