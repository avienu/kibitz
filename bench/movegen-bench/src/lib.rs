//! Shared fixtures and per-library kernels for the Phase 0 GO/NO-GO benchmark.
//!
//! GPL-3.0 (depends on shakmaty). Never a dependency of anything else.

/// (name, FEN) pairs spanning opening, tactical middlegame, quiet middlegame,
/// and endgame so neither library is measured on a single position character.
/// Sources: startpos; Chess Programming Wiki perft positions 2, 3, 6.
pub const BENCH_FENS: &[(&str, &str)] = &[
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    ),
    ("cpw3_endgame", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    (
        "cpw6_middlegame",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1b1/2B1P1B1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ),
];

pub mod cozy {
    use cozy_chess::{
        get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves,
        BitBoard, Board, Color, Piece, Square,
    };

    pub fn parse(fen: &str) -> Board {
        fen.parse().expect("valid FEN")
    }

    /// Generate all legal moves and return the count.
    pub fn count_moves(board: &Board) -> u64 {
        let mut n = 0u64;
        board.generate_moves(|moves| {
            n += moves.len() as u64;
            false
        });
        n
    }

    /// For every square, compute the full attackers-to-square set for both
    /// colors and return the summed population count.
    pub fn attackers_all_squares(board: &Board) -> u32 {
        let occ = board.occupied();
        let mut total = 0u32;
        for sq in Square::ALL {
            let mut attackers = BitBoard::EMPTY;
            attackers |= get_knight_moves(sq) & board.pieces(Piece::Knight);
            attackers |= get_king_moves(sq) & board.pieces(Piece::King);
            attackers |=
                get_rook_moves(sq, occ) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
            attackers |= get_bishop_moves(sq, occ)
                & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen));
            attackers |= get_pawn_attacks(sq, Color::White)
                & board.pieces(Piece::Pawn)
                & board.colors(Color::Black);
            attackers |= get_pawn_attacks(sq, Color::Black)
                & board.pieces(Piece::Pawn)
                & board.colors(Color::White);
            total += attackers.len();
        }
        total
    }

    pub fn perft(board: &Board, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let mut nodes = 0u64;
        board.generate_moves(|moves| {
            if depth == 1 {
                nodes += moves.len() as u64;
            } else {
                for mv in moves {
                    let mut child = board.clone();
                    child.play_unchecked(mv);
                    nodes += perft(&child, depth - 1);
                }
            }
            false
        });
        nodes
    }
}

pub mod shak {
    use shakmaty::{fen::Fen, CastlingMode, Chess, Color, Position, Square};

    #[allow(clippy::result_large_err)]
    pub fn parse(fen: &str) -> Chess {
        // CPW perft position 6 trips shakmaty's strict material validator
        // (two same-colored bishops with all eight pawns), so allow that.
        fen.parse::<Fen>()
            .unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"))
            .into_position(CastlingMode::Standard)
            .or_else(|e| e.ignore_too_much_material())
            .unwrap_or_else(|e| panic!("illegal position {fen}: {e}"))
    }

    /// Generate all legal moves and return the count.
    pub fn count_moves(pos: &Chess) -> u64 {
        pos.legal_moves().len() as u64
    }

    /// For every square, compute the full attackers-to-square set for both
    /// colors and return the summed population count.
    pub fn attackers_all_squares(pos: &Chess) -> u32 {
        let board = pos.board();
        let occ = board.occupied();
        let mut total = 0u32;
        for sq in Square::ALL {
            total += board.attacks_to(sq, Color::White, occ).count() as u32;
            total += board.attacks_to(sq, Color::Black, occ).count() as u32;
        }
        total
    }

    pub fn perft(pos: &Chess, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = pos.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        moves
            .iter()
            .map(|m| {
                let mut child = pos.clone();
                child.play_unchecked(m);
                perft(&child, depth - 1)
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both kernels must agree with each other and with known perft values,
    /// otherwise the benchmark compares different work.
    #[test]
    fn kernels_agree() {
        for (name, fen) in BENCH_FENS {
            let cb = cozy::parse(fen);
            let sp = shak::parse(fen);
            assert_eq!(
                cozy::count_moves(&cb),
                shak::count_moves(&sp),
                "movegen count mismatch on {name}"
            );
            assert_eq!(
                cozy::attackers_all_squares(&cb),
                shak::attackers_all_squares(&sp),
                "attackers count mismatch on {name}"
            );
            assert_eq!(
                cozy::perft(&cb, 3),
                shak::perft(&sp, 3),
                "perft(3) mismatch on {name}"
            );
        }
    }
}
