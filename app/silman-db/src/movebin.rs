//! Binary move encoding: one byte per ply.
//!
//! Each byte is the move's index in a *fully specified* deterministic
//! ordering of the legal moves: ascending by (from square, to square,
//! promotion piece), with squares ordered a1=0 … h8=63 and promotions
//! None < Knight < Bishop < Rook < Queen. The ordering therefore depends
//! only on the legal move *set* (not on cozy-chess's internal generation
//! order), so any correct move generator can decode it.
//!
//! `ENCODING_VERSION` is stored in the database; bump it if this ordering
//! rule ever changes.

use cozy_chess::{Board, Move, Piece};

pub const ENCODING_VERSION: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum MoveBinError {
    #[error("move {0} is not legal in the current position")]
    IllegalMove(String),
    #[error("byte {byte} out of range: position has {legal} legal moves (ply {ply})")]
    IndexOutOfRange { byte: u8, legal: usize, ply: usize },
}

fn promo_key(p: Option<Piece>) -> u8 {
    match p {
        None => 0,
        Some(Piece::Knight) => 1,
        Some(Piece::Bishop) => 2,
        Some(Piece::Rook) => 3,
        Some(Piece::Queen) => 4,
        // Kings/pawns can't be promotion targets; order them last defensively.
        Some(_) => 5,
    }
}

/// The deterministic legal-move ordering the encoding indexes into.
pub fn ordered_legal_moves(board: &Board) -> Vec<Move> {
    let mut moves = Vec::with_capacity(64);
    board.generate_moves(|pm| {
        moves.extend(pm);
        false
    });
    moves.sort_by_key(|m| (m.from as u8, m.to as u8, promo_key(m.promotion)));
    moves
}

/// Encode `moves` (played from `start`) as one byte per ply.
pub fn encode_game(start: &Board, moves: &[Move]) -> Result<Vec<u8>, MoveBinError> {
    let mut board = start.clone();
    let mut out = Vec::with_capacity(moves.len());
    for &mv in moves {
        let ordered = ordered_legal_moves(&board);
        let idx = ordered
            .iter()
            .position(|&m| m == mv)
            .ok_or_else(|| MoveBinError::IllegalMove(format!("{mv}")))?;
        debug_assert!(idx < 256, "chess positions have < 256 legal moves");
        out.push(idx as u8);
        board.play(mv);
    }
    Ok(out)
}

/// Decode a byte string produced by [`encode_game`], returning the moves.
pub fn decode_game(start: &Board, bytes: &[u8]) -> Result<Vec<Move>, MoveBinError> {
    let mut board = start.clone();
    let mut out = Vec::with_capacity(bytes.len());
    for (ply, &byte) in bytes.iter().enumerate() {
        let ordered = ordered_legal_moves(&board);
        let mv = *ordered
            .get(byte as usize)
            .ok_or(MoveBinError::IndexOutOfRange {
                byte,
                legal: ordered.len(),
                ply,
            })?;
        out.push(mv);
        board.play(mv);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::san::parse_san;

    #[test]
    fn encode_decode_round_trip_opera_game() {
        let sans = "e4 e5 Nf3 d6 d4 Bg4 dxe5 Bxf3 Qxf3 dxe5 Bc4 Nf6 Qb3 Qe7 \
                    Nc3 c6 Bg5 b5 Nxb5 cxb5 Bxb5+ Nbd7 O-O-O Rd8 Rxd7 Rxd7 \
                    Rd1 Qe6 Bxd7+ Nxd7 Qb8+ Nxb8 Rd8#";
        let start = Board::default();
        let mut board = start.clone();
        let mut moves = Vec::new();
        for san in sans.split_whitespace() {
            let mv = parse_san(&board, san).unwrap();
            moves.push(mv);
            board.play(mv);
        }
        let bytes = encode_game(&start, &moves).unwrap();
        assert_eq!(bytes.len(), moves.len());
        assert_eq!(decode_game(&start, &bytes).unwrap(), moves);
    }

    #[test]
    fn out_of_range_byte_is_an_error() {
        let start = Board::default();
        assert!(matches!(
            decode_game(&start, &[255]),
            Err(MoveBinError::IndexOutOfRange { .. })
        ));
    }
}
