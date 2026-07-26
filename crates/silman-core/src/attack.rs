//! Attack/defense map helpers shared by all detectors.

use cozy_chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    Board, Color, Piece, Square,
};

/// All pieces of `by` attacking `sq`, given `occ` as the blocker set.
pub fn attackers_of(board: &Board, sq: Square, by: Color, occ: BitBoard) -> BitBoard {
    let side = board.colors(by);
    let mut a = BitBoard::EMPTY;
    a |= get_knight_moves(sq) & board.pieces(Piece::Knight);
    a |= get_king_moves(sq) & board.pieces(Piece::King);
    a |= get_rook_moves(sq, occ) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
    a |= get_bishop_moves(sq, occ) & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen));
    // Pawns of `by` that attack `sq` sit on the squares a pawn of the
    // OTHER color on `sq` would attack.
    a |= get_pawn_attacks(sq, !by) & board.pieces(Piece::Pawn);
    a & side
}

/// Squares attacked by any piece of `by` (full occupancy).
pub fn attacked_squares(board: &Board, by: Color) -> BitBoard {
    let occ = board.occupied();
    let mut a = BitBoard::EMPTY;
    for sq in board.colored_pieces(by, Piece::Pawn) {
        a |= get_pawn_attacks(sq, by);
    }
    for sq in board.colored_pieces(by, Piece::Knight) {
        a |= get_knight_moves(sq);
    }
    for sq in board.colored_pieces(by, Piece::King) {
        a |= get_king_moves(sq);
    }
    for sq in board.colored_pieces(by, Piece::Rook) | board.colored_pieces(by, Piece::Queen) {
        a |= get_rook_moves(sq, occ);
    }
    for sq in board.colored_pieces(by, Piece::Bishop) | board.colored_pieces(by, Piece::Queen) {
        a |= get_bishop_moves(sq, occ);
    }
    a
}

/// Pieces of `color` absolutely pinned to their own king.
pub fn pinned_pieces(board: &Board, color: Color) -> BitBoard {
    let king = board.king(color);
    let occ = board.occupied();
    let own = board.colors(color);
    let enemy = !color;
    let mut pinned = BitBoard::EMPTY;

    // X-ray: enemy sliders aligned with the king (empty-board rays); a
    // single piece between such a sniper and the king, belonging to
    // `color`, is absolutely pinned.
    let rook_like = (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)) & board.colors(enemy);
    let bishop_like =
        (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)) & board.colors(enemy);
    let snipers = (rook_like & get_rook_moves(king, BitBoard::EMPTY))
        | (bishop_like & get_bishop_moves(king, BitBoard::EMPTY));
    for sniper in snipers {
        let between = cozy_chess::get_between_rays(sniper, king) & occ;
        if between.len() == 1 {
            pinned |= between & own;
        }
    }
    pinned
}

/// May a pinned piece of `color` on `from` still act on `target`? (A pinned
/// piece can capture/defend only along the ray through its own king.)
pub fn pinned_piece_covers(board: &Board, color: Color, from: Square, target: Square) -> bool {
    let king = board.king(color);
    cozy_chess::get_line_rays(king, from).has(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(fen: &str) -> Board {
        fen.parse().unwrap()
    }

    #[test]
    fn attackers_and_pins() {
        // Ruy Lopez after 4...d6: the c6 knight IS absolutely pinned by
        // Ba4 (d7 is empty; b5-c6-d7 ray holds only the knight).
        let b = board("r1bqkbnr/1pp2ppp/p1np4/4p3/B3P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 5");
        let pinned = pinned_pieces(&b, Color::Black);
        assert!(pinned.has(Square::C6), "c6 knight is pinned");
        assert_eq!(pinned.len(), 1);

        // e5 pawn attackers: white Nf3; defenders include black Nc6.
        let occ = b.occupied();
        assert!(attackers_of(&b, Square::E5, Color::White, occ).has(Square::F3));
        assert!(attackers_of(&b, Square::E5, Color::Black, occ).has(Square::C6));

        // The pinned knight covers squares only along the a4-e8 ray.
        assert!(pinned_piece_covers(
            &b,
            Color::Black,
            Square::C6,
            Square::D7
        ));
        assert!(!pinned_piece_covers(
            &b,
            Color::Black,
            Square::C6,
            Square::E5
        ));
    }
}
