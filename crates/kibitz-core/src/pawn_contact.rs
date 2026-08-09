//! Pawn contact: how soon can a side's PAWNS attack a given square?
//!
//! The existing detectors ask only "is this square attacked by a pawn
//! right now?". That is enough to judge a one-move landing but useless for
//! a multi-move maneuver: a waypoint nobody attacks today is worthless if
//! the opponent can push a pawn at it in the same number of moves the
//! knight needs to arrive. Jeremy Silman's whole objection to "outposts" that can
//! be kicked (Complete Book of Chess Strategy p. 219) is a statement about
//! pawn contact over TIME, not about the current attack map.
//!
//! [`evict_distance`] answers it as a distance map: for every square, the
//! minimum number of pawn moves `side` needs before one of its pawns
//! attacks that square. Destination squares are still judged by the
//! permanent [`crate::imbalance`] hole test (a hole is a square no enemy
//! pawn can EVER attack); this map is for everything in between.

use cozy_chess::{get_pawn_attacks, Board, Color, Piece, Rank, Square};

/// No pawn of this side can ever attack the square (within the model).
pub const NEVER: u8 = u8::MAX;

/// For each square, the minimum number of pawn moves `side` must make
/// before one of its pawns attacks it. `0` means already attacked.
///
/// Model (v1, deliberately conservative — it UNDER-states what pawns can
/// do, so plans built on it are cautious rather than speculative):
///
/// - Quiet pushes only. A pawn that would have to CAPTURE to reach the
///   attacking square is not counted, since the capture depends on a
///   target that may not still be there.
/// - The double push from the home rank costs one move, not two.
/// - Any occupied square in front of a pawn stops that pawn's walk. Pieces
///   do move away, so this under-states reach; pawn blockades are genuinely
///   permanent and this models them exactly.
pub fn evict_distance(board: &Board, side: Color) -> [u8; 64] {
    let mut dist = [NEVER; 64];
    let dr: i8 = match side {
        Color::White => 1,
        Color::Black => -1,
    };
    let home = match side {
        Color::White => Rank::Second,
        Color::Black => Rank::Seventh,
    };
    for pawn in board.colored_pieces(side, Piece::Pawn) {
        let from_home = pawn.rank() == home;
        let mut sq = pawn;
        let mut steps: u8 = 0;
        loop {
            // Cost in MOVES to have the pawn standing on `sq`. The double
            // push covers the first two squares for a single move.
            let cost = if from_home && steps >= 2 {
                steps - 1
            } else {
                steps
            };
            for a in get_pawn_attacks(sq, side) {
                let slot = &mut dist[a as usize];
                if cost < *slot {
                    *slot = cost;
                }
            }
            let Some(next) = sq.try_offset(0, dr) else {
                break;
            };
            if board.occupied().has(next) {
                break;
            }
            sq = next;
            steps += 1;
        }
    }
    dist
}

/// True if `dist` (a map from [`evict_distance`]) says the square can be
/// attacked by a pawn within `moves` pawn moves.
pub fn contested_within(dist: &[u8; 64], sq: Square, moves: u8) -> bool {
    dist[sq as usize] <= moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(fen: &str, side: Color) -> [u8; 64] {
        evict_distance(&Board::from_str(fen).expect("fen"), side)
    }

    /// The opening position: White's pawns attack the third rank now (0)
    /// and the fourth rank after one move (the double push).
    #[test]
    fn start_position_distances() {
        let dist = d(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            Color::White,
        );
        assert_eq!(dist[Square::C3 as usize], 0, "b2/d2 attack c3 already");
        assert_eq!(dist[Square::C4 as usize], 1, "b2-b4 attacks c5 in one");
        assert_eq!(dist[Square::C5 as usize], 1, "the double push reaches");
        assert_eq!(dist[Square::C6 as usize], 2, "b4-b5 is a second move");
    }

    /// The kickable knight: a knight on g5 with an enemy h-pawn on h7 is
    /// one pawn move from being hit (h7-h6). This is the case the old
    /// current-attacks-only test called safe. (Jeremy Silman, CBoCS p. 219 —
    /// an "outpost" a pawn can evict is not an outpost.)
    #[test]
    fn knight_on_g5_is_one_move_from_being_kicked() {
        let dist = d(
            "rnbqkb1r/pppppppp/8/6N1/8/8/PPPPPPPP/RNBQKB1R b KQkq - 0 1",
            Color::Black,
        );
        assert_eq!(dist[Square::G5 as usize], 1, "h7-h6 hits g5");
        assert_eq!(dist[Square::E5 as usize], 1, "d7-d6 and f7-f6 both hit");
    }

    /// A pawn blockade is permanent: the blocked pawn's walk stops, so
    /// squares beyond the blockade are NEVER reachable by that pawn.
    #[test]
    fn blocked_pawn_never_reaches_past_the_blockade() {
        // White d4 is met by a black d5 pawn; neither can advance.
        let dist = d("4k3/8/8/3p4/3P4/8/8/4K3 w - - 0 1", Color::White);
        assert_eq!(dist[Square::C5 as usize], 0, "d4 already attacks c5");
        assert_eq!(
            dist[Square::C6 as usize],
            NEVER,
            "d4 can never advance past the d5 blockade"
        );
    }

    /// A true hole is exactly a square with no pawn contact at any
    /// distance: Black has no c- or e-pawn, so d6 can never be attacked.
    #[test]
    fn permanent_hole_is_never_contested() {
        let dist = d("4k3/pp3pp1/8/3n4/8/8/PPPPPPPP/4K3 b - - 0 1", Color::Black);
        assert_eq!(dist[Square::D6 as usize], NEVER);
        assert_eq!(dist[Square::D5 as usize], NEVER);
        assert!(!contested_within(&dist, Square::D5, 5));
    }

    /// The timing question the router actually asks: a square is unsafe
    /// for a piece arriving in `h` moves only if a pawn can reach it in
    /// `h` moves or fewer.
    #[test]
    fn contested_within_is_a_timing_test() {
        let dist = d(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            Color::White,
        );
        // c6 needs two white pawn moves (b2-b4-b5).
        assert!(!contested_within(&dist, Square::C6, 1));
        assert!(contested_within(&dist, Square::C6, 2));
    }
}
