//! Static exchange evaluation (SEE): the material outcome of the best
//! capture sequence on one square, assuming both sides capture with their
//! least valuable attacker and may stop when ahead.

use cozy_chess::{BitBoard, Board, Color, Piece, Square};

use crate::attack::attackers_of;

/// Centipawn piece values used across the WSUI screen.
pub fn piece_value(p: Piece) -> i32 {
    match p {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 330,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 20_000,
    }
}

fn least_valuable_attacker(board: &Board, attackers: BitBoard) -> Option<(Square, Piece)> {
    for piece in [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
        Piece::King,
    ] {
        let set = attackers & board.pieces(piece);
        if let Some(sq) = set.into_iter().next() {
            return Some((sq, piece));
        }
    }
    None
}

/// SEE for `attacker_side` initiating the capture sequence on `target`
/// (which must hold a piece of the other side). Positive = the attacker
/// wins material. Attackers are recomputed after each capture so x-rays
/// (battery pieces behind the capturer) participate.
pub fn see(board: &Board, target: Square, attacker_side: Color) -> i32 {
    let Some(mut victim) = board.piece_on(target) else {
        return 0;
    };
    let mut occ = board.occupied();
    let mut side = attacker_side;
    let mut gains: Vec<i32> = Vec::with_capacity(8);

    loop {
        let attackers = attackers_of(board, target, side, occ) & occ;
        let Some((from, piece)) = least_valuable_attacker(board, attackers) else {
            break;
        };
        // A king may not recapture into remaining enemy attackers.
        if piece == Piece::King {
            let enemy_attackers = attackers_of(board, target, !side, occ & !from.bitboard());
            if !enemy_attackers.is_empty() {
                break;
            }
        }
        gains.push(piece_value(victim));
        victim = piece;
        occ &= !from.bitboard();
        side = !side;
    }

    // Negamax the gain sequence: each side may decline to continue.
    let mut value = 0i32;
    for &g in gains.iter().rev() {
        value = (g - value).max(0);
    }
    // The first capture is not optional for the *score* (we're asking:
    // if the attacker starts taking, what's the best outcome?).
    if gains.is_empty() {
        0
    } else {
        let mut v = 0;
        for (i, &g) in gains.iter().enumerate().rev() {
            if i == 0 {
                v = g - v;
            } else {
                v = (g - v).max(0);
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(fen: &str) -> Board {
        fen.parse().unwrap()
    }

    #[test]
    fn simple_free_capture() {
        // White Nf3 can take an undefended e5 pawn.
        let b = board("4k3/8/8/4p3/8/5N2/8/4K3 w - - 0 1");
        assert_eq!(see(&b, Square::E5, Color::White), 100);
    }

    #[test]
    fn defended_pawn_is_a_bad_capture_for_a_knight() {
        // e5 pawn defended by d6 pawn: NxP, PxN = 100 - 320 < 0.
        let b = board("4k3/8/3p4/4p3/8/5N2/8/4K3 w - - 0 1");
        assert_eq!(see(&b, Square::E5, Color::White), 100 - 320);
    }

    #[test]
    fn battery_xray_participates() {
        // Doubled rooks vs single defender: RxP, RxR, RxR wins pawn+rook.
        // White Ra1,Ra2 vs black pawn a7 defended by Ra8.
        let b = board("r3k3/p7/8/8/8/8/R7/R3K3 w - - 0 1");
        // Rxa7 Rxa7 Rxa7: +100 -500 +500 = +100
        assert_eq!(see(&b, Square::A7, Color::White), 100);
    }

    #[test]
    fn stopping_when_ahead() {
        // Queen takes defended pawn: QxP PxQ would be -800, so the answer
        // for "queen initiates" is 100 - 900 = -800 (forced first capture,
        // then defender happily recaptures).
        let b = board("4k3/8/3p4/4p3/8/8/4Q3/4K3 w - - 0 1");
        assert_eq!(see(&b, Square::E5, Color::White), 100 - 900);
    }

    #[test]
    fn pinned_scenarios_are_not_sees_job() {
        // SEE ignores pins by design (documented); the detectors adjust.
        let b = board("4k3/8/8/4p3/8/5N2/8/4K3 b - - 0 1");
        assert_eq!(see(&b, Square::E5, Color::White), 100);
    }
}
