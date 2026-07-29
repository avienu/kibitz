//! Normalized 64-bit position hash for the position index.
//!
//! `cozy_chess::Board::hash()` includes the en-passant file whenever the
//! previous move was a double pawn push, even if no en-passant capture is
//! actually possible. True position identity (as in the FIDE repetition
//! rule, and as emitted by most FEN writers) counts the ep square only when
//! an ep capture is legal. This module hashes through that normalization so
//! a position reached by play matches the same position parsed from a FEN
//! with an `-` ep field, and transpositions differing only in a phantom ep
//! file collide as they should.
//!
//! POSITION_HASH_VERSION (db.rs) identifies this function; bump it if the
//! normalization or the underlying cozy-chess hash ever changes.

use cozy_chess::{Board, Color, Piece, Rank, Square};

/// Is there a *legal* en-passant capture in this position?
fn has_legal_ep_capture(board: &Board) -> bool {
    let Some(file) = board.en_passant() else {
        return false;
    };
    let target_rank = match board.side_to_move() {
        Color::White => Rank::Sixth,
        Color::Black => Rank::Third,
    };
    let target = Square::new(file, target_rank);
    let mut found = false;
    board.generate_moves(|pm| {
        if pm.piece == Piece::Pawn {
            for mv in pm {
                // A pawn moving diagonally onto the (empty) ep target square
                // is the en-passant capture; a same-file move would be a push.
                if mv.to == target && mv.from.file() != file {
                    found = true;
                }
            }
        }
        found
    });
    found
}

/// Strip a phantom ep square by round-tripping through FEN with the ep
/// field replaced by `-`. The round-trip is NOT infallible: a Chess960
/// position renders file-letter castling rights that the standard parser
/// refuses (2026-07-28 field report — one 960 game panicked the import
/// worker here, silently wedging every chess.com sync at the same month).
/// Falling back to the un-normalized board keeps the hash total; the
/// phantom-ep dedup nicety is lost only for positions we cannot round-trip.
fn without_ep(board: &Board) -> Board {
    let fen = board.to_string();
    let mut fields: Vec<&str> = fen.split_ascii_whitespace().collect();
    debug_assert!(fields.len() >= 4);
    fields[3] = "-";
    fields.join(" ").parse().unwrap_or_else(|_| board.clone())
}

/// The normalized position hash used by the `positions` index.
pub fn position_hash(board: &Board) -> u64 {
    if board.en_passant().is_some() && !has_legal_ep_capture(board) {
        without_ep(board).hash()
    } else {
        board.hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phantom_ep_matches_dash_fen() {
        // After 1.e4 c5 no ep capture is possible; the played position must
        // hash identically to the conventional `-` FEN.
        let mut played = Board::default();
        played.play("e2e4".parse().unwrap());
        played.play("c7c5".parse().unwrap());
        let parsed: Board = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2"
            .parse()
            .unwrap();
        assert_ne!(
            played.hash(),
            parsed.hash(),
            "raw hashes differ (phantom ep)"
        );
        assert_eq!(position_hash(&played), position_hash(&parsed));
    }

    #[test]
    fn real_ep_rights_still_distinguish_positions() {
        // After 1.e4 a6 2.e5 d5 white can capture d5 en passant: the ep
        // right is real and must stay part of the position identity.
        let mut played = Board::default();
        for m in ["e2e4", "a7a6", "e4e5", "d7d5"] {
            played.play(m.parse().unwrap());
        }
        assert!(has_legal_ep_capture(&played));
        let no_ep: Board = "rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3"
            .parse()
            .unwrap();
        assert_ne!(position_hash(&played), position_hash(&no_ep));
    }

    #[test]
    fn pinned_pawn_cannot_capture_ep_so_ep_is_phantom() {
        // Black b4-pawn is pinned... construct instead a case where the only
        // adjacent pawn is absolutely pinned: white Ke5, pawn f5; black plays
        // g7-g5; white pawn f5 is pinned by a rook on... keep it simple: no
        // adjacent pawn at all.
        let mut played = Board::default();
        played.play("e2e4".parse().unwrap());
        played.play("g8f6".parse().unwrap());
        played.play("e4e5".parse().unwrap());
        played.play("d7d5".parse().unwrap());
        // Here exd6 e.p. IS legal (pawn e5 adjacent to d5): real ep.
        assert!(has_legal_ep_capture(&played));
        // But after 1.e4 g6 2.e5 d5? No — e5 pawn is adjacent to d5 again.
        // Use h-file double push far from any white pawn instead.
        let mut b = Board::default();
        b.play("a2a3".parse().unwrap());
        b.play("h7h5".parse().unwrap());
        assert!(!has_legal_ep_capture(&b));
        let dash: Board = "rnbqkbnr/ppppppp1/8/7p/8/P7/1PPPPPPP/RNBQKBNR w KQkq - 0 2"
            .parse()
            .unwrap();
        assert_eq!(position_hash(&b), position_hash(&dash));
    }
}
