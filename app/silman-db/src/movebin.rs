//! Binary movetext encoding.
//!
//! ## Version 2 (current): move indices + inline escape tokens
//!
//! Chess positions have at most 218 legal moves, so byte values 0..=217
//! are the move's index in a *fully specified* deterministic ordering of
//! the legal moves: ascending by (from square, to square, promotion
//! piece), squares a1=0 … h8=63, promotions None < Knight < Bishop <
//! Rook < Queen. The ordering depends only on the legal move *set*, so
//! any correct move generator can decode it.
//!
//! The top of the byte range carries escape tokens, keeping the stream
//! single-pass, annotations physically local to their moves, and
//! unannotated games at exactly one byte per ply (+ END):
//!
//! | byte | token |
//! |------|-------|
//! | 255  | ESCAPE (reserved for future extension) |
//! | 254  | END (end of stream) |
//! | 253  | VAR_START (variation replacing the preceding move; nestable) |
//! | 252  | VAR_END |
//! | 251  | NAG (next byte = NAG value) |
//! | 250  | COMMENT (next: LEB128 length + UTF-8 bytes) |
//! | 249  | NULL_MOVE (side-to-move flip, en-passant cleared) |
//! | 218–248 | reserved (decode error) |
//!
//! Token order mirrors PGN text order: a move, then NAG/comment attached
//! to it, then variations, then the next move. A COMMENT before any move
//! belongs to the game (or variation) start.
//!
//! Null moves while in check cannot be represented as a legal cozy-chess
//! position; importers truncate the affected line at that point rather
//! than failing the game (see DECISIONS_NEEDED.md item 2, decided
//! 2026-07-25).
//!
//! ## Version 1 (legacy, upgraded on open)
//!
//! Bare move indices, mainline only, no tokens. `decode_game_v1` exists
//! solely for the one-shot v1→v2 database upgrade in `db::open`.

use cozy_chess::{Board, Move, Piece};

pub const ENCODING_VERSION: u16 = 2;

pub const TOK_ESCAPE: u8 = 255;
pub const TOK_END: u8 = 254;
pub const TOK_VAR_START: u8 = 253;
pub const TOK_VAR_END: u8 = 252;
pub const TOK_NAG: u8 = 251;
pub const TOK_COMMENT: u8 = 250;
pub const TOK_NULL: u8 = 249;
/// Largest valid move index (218 legal moves max).
pub const MAX_MOVE_INDEX: u8 = 217;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Move(Move),
    Null,
    Nag(u8),
    Comment(String),
    VarStart,
    VarEnd,
}

/// One mainline ply: a real move or a null ("pass").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ply {
    Move(Move),
    Null,
}

#[derive(Debug, thiserror::Error)]
pub enum MoveBinError {
    #[error("move {0} is not legal in the current position")]
    IllegalMove(String),
    #[error("byte {byte} out of range: position has {legal} legal moves (ply {ply})")]
    IndexOutOfRange { byte: u8, legal: usize, ply: usize },
    #[error("reserved byte {0} in movetext stream")]
    ReservedByte(u8),
    #[error("stream ended without END token")]
    MissingEnd,
    #[error("truncated token stream while reading {0}")]
    Truncated(&'static str),
    #[error("variation structure invalid: {0}")]
    Variation(&'static str),
    #[error("null move while in check is not representable")]
    NullInCheck,
    #[error("comment is not valid UTF-8")]
    CommentUtf8,
    #[error("mainline contains a null move; caller cannot represent it")]
    MainlineNull,
}

fn promo_key(p: Option<Piece>) -> u8 {
    match p {
        None => 0,
        Some(Piece::Knight) => 1,
        Some(Piece::Bishop) => 2,
        Some(Piece::Rook) => 3,
        Some(Piece::Queen) => 4,
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

fn push_varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn read_varint(bytes: &[u8], pos: &mut usize) -> Result<u32, MoveBinError> {
    let mut v: u32 = 0;
    let mut shift = 0;
    loop {
        let b = *bytes.get(*pos).ok_or(MoveBinError::Truncated("varint"))?;
        *pos += 1;
        v |= ((b & 0x7F) as u32) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift > 28 {
            return Err(MoveBinError::Truncated("varint too long"));
        }
    }
}

/// Replay state for encode/decode: current board plus the board before the
/// last move at this nesting level (the branch point for variations).
#[derive(Clone)]
struct Level {
    cur: Board,
    before_last: Option<Board>,
}

/// Encode a token stream (moves, annotations, variations) played from
/// `start` into version-2 bytes.
pub fn encode_tokens(start: &Board, tokens: &[Token]) -> Result<Vec<u8>, MoveBinError> {
    let mut out = Vec::with_capacity(tokens.len() + 1);
    let mut level = Level {
        cur: start.clone(),
        before_last: None,
    };
    let mut stack: Vec<Level> = Vec::new();

    for token in tokens {
        match token {
            Token::Move(mv) => {
                let ordered = ordered_legal_moves(&level.cur);
                let idx = ordered
                    .iter()
                    .position(|m| m == mv)
                    .ok_or_else(|| MoveBinError::IllegalMove(format!("{mv}")))?;
                out.push(idx as u8);
                level.before_last = Some(level.cur.clone());
                level.cur.play(*mv);
            }
            Token::Null => {
                out.push(TOK_NULL);
                let next = level.cur.null_move().ok_or(MoveBinError::NullInCheck)?;
                level.before_last = Some(level.cur.clone());
                level.cur = next;
            }
            Token::Nag(n) => {
                out.push(TOK_NAG);
                out.push(*n);
            }
            Token::Comment(text) => {
                out.push(TOK_COMMENT);
                push_varint(&mut out, text.len() as u32);
                out.extend_from_slice(text.as_bytes());
            }
            Token::VarStart => {
                let branch = level
                    .before_last
                    .clone()
                    .ok_or(MoveBinError::Variation("variation before any move"))?;
                out.push(TOK_VAR_START);
                stack.push(level.clone());
                level = Level {
                    cur: branch,
                    before_last: None,
                };
            }
            Token::VarEnd => {
                out.push(TOK_VAR_END);
                level = stack
                    .pop()
                    .ok_or(MoveBinError::Variation("VAR_END without VAR_START"))?;
            }
        }
    }
    if !stack.is_empty() {
        return Err(MoveBinError::Variation("unclosed variation"));
    }
    out.push(TOK_END);
    Ok(out)
}

/// Decode version-2 bytes into the token stream.
pub fn decode_tokens(start: &Board, bytes: &[u8]) -> Result<Vec<Token>, MoveBinError> {
    let mut tokens = Vec::new();
    let mut level = Level {
        cur: start.clone(),
        before_last: None,
    };
    let mut stack: Vec<Level> = Vec::new();
    let mut pos = 0usize;
    let mut ply = 0usize;

    loop {
        let b = *bytes.get(pos).ok_or(MoveBinError::MissingEnd)?;
        pos += 1;
        match b {
            0..=MAX_MOVE_INDEX => {
                let ordered = ordered_legal_moves(&level.cur);
                let mv = *ordered
                    .get(b as usize)
                    .ok_or(MoveBinError::IndexOutOfRange {
                        byte: b,
                        legal: ordered.len(),
                        ply,
                    })?;
                tokens.push(Token::Move(mv));
                level.before_last = Some(level.cur.clone());
                level.cur.play(mv);
                ply += 1;
            }
            TOK_NULL => {
                let next = level.cur.null_move().ok_or(MoveBinError::NullInCheck)?;
                tokens.push(Token::Null);
                level.before_last = Some(level.cur.clone());
                level.cur = next;
                ply += 1;
            }
            TOK_NAG => {
                let n = *bytes.get(pos).ok_or(MoveBinError::Truncated("NAG"))?;
                pos += 1;
                tokens.push(Token::Nag(n));
            }
            TOK_COMMENT => {
                let len = read_varint(bytes, &mut pos)? as usize;
                let end = pos + len;
                let slice = bytes
                    .get(pos..end)
                    .ok_or(MoveBinError::Truncated("comment bytes"))?;
                pos = end;
                let text =
                    String::from_utf8(slice.to_vec()).map_err(|_| MoveBinError::CommentUtf8)?;
                tokens.push(Token::Comment(text));
            }
            TOK_VAR_START => {
                let branch = level
                    .before_last
                    .clone()
                    .ok_or(MoveBinError::Variation("variation before any move"))?;
                tokens.push(Token::VarStart);
                stack.push(level.clone());
                level = Level {
                    cur: branch,
                    before_last: None,
                };
            }
            TOK_VAR_END => {
                tokens.push(Token::VarEnd);
                level = stack
                    .pop()
                    .ok_or(MoveBinError::Variation("VAR_END without VAR_START"))?;
            }
            TOK_END => {
                if !stack.is_empty() {
                    return Err(MoveBinError::Variation("END inside a variation"));
                }
                return Ok(tokens);
            }
            TOK_ESCAPE => return Err(MoveBinError::ReservedByte(b)),
            _ => return Err(MoveBinError::ReservedByte(b)),
        }
    }
}

/// Extract the mainline (depth-0) plies from a token stream.
pub fn mainline_of(tokens: &[Token]) -> Vec<Ply> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    for t in tokens {
        match t {
            Token::VarStart => depth += 1,
            Token::VarEnd => depth = depth.saturating_sub(1),
            Token::Move(mv) if depth == 0 => out.push(Ply::Move(*mv)),
            Token::Null if depth == 0 => out.push(Ply::Null),
            _ => {}
        }
    }
    out
}

/// Decode the mainline plies (moves and nulls) of a v2 stream.
pub fn decode_mainline(start: &Board, bytes: &[u8]) -> Result<Vec<Ply>, MoveBinError> {
    Ok(mainline_of(&decode_tokens(start, bytes)?))
}

/// Decode the mainline as plain moves. Errors if the mainline contains a
/// null move — callers that can render nulls should use `decode_mainline`.
pub fn decode_game(start: &Board, bytes: &[u8]) -> Result<Vec<Move>, MoveBinError> {
    decode_mainline(start, bytes)?
        .into_iter()
        .map(|p| match p {
            Ply::Move(m) => Ok(m),
            Ply::Null => Err(MoveBinError::MainlineNull),
        })
        .collect()
}

/// Encode a bare mainline (no annotations) — the common bulk-import path.
pub fn encode_game(start: &Board, moves: &[Move]) -> Result<Vec<u8>, MoveBinError> {
    let tokens: Vec<Token> = moves.iter().map(|&m| Token::Move(m)).collect();
    encode_tokens(start, &tokens)
}

/// Legacy version-1 decoder (bare indices, no END token). Used only by the
/// one-shot database upgrade.
pub fn decode_game_v1(start: &Board, bytes: &[u8]) -> Result<Vec<Move>, MoveBinError> {
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

    fn opera_moves() -> (Board, Vec<Move>) {
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
        (start, moves)
    }

    #[test]
    fn bare_mainline_round_trip_and_density() {
        let (start, moves) = opera_moves();
        let bytes = encode_game(&start, &moves).unwrap();
        assert_eq!(bytes.len(), moves.len() + 1, "1 byte/ply + END");
        assert_eq!(decode_game(&start, &bytes).unwrap(), moves);
    }

    #[test]
    fn annotated_token_stream_round_trips() {
        let start = Board::default();
        let b = |san: &str, board: &Board| parse_san(board, san).unwrap();
        // 1. e4 {best by test} e5 (1... c5 $1 2. Nf3 (2. c3)) 2. Nf3 --
        let mut board = start.clone();
        let e4 = b("e4", &board);
        board.play(e4);
        let board_after_e4 = board.clone();
        let e5 = b("e5", &board);
        board.play(e5);
        let nf3 = b("Nf3", &board);
        // variation boards
        let c5 = b("c5", &board_after_e4);
        let mut var = board_after_e4.clone();
        var.play(c5);
        let var_nf3 = b("Nf3", &var);
        let var_c3 = b("c3", &var);

        let tokens = vec![
            Token::Comment("pre-game".into()),
            Token::Move(e4),
            Token::Comment("best by test".into()),
            Token::Move(e5),
            Token::VarStart,
            Token::Move(c5),
            Token::Nag(1),
            Token::Move(var_nf3),
            Token::VarStart,
            Token::Move(var_c3),
            Token::VarEnd,
            Token::VarEnd,
            Token::Move(nf3),
            Token::Null,
        ];
        let bytes = encode_tokens(&start, &tokens).unwrap();
        let decoded = decode_tokens(&start, &bytes).unwrap();
        assert_eq!(decoded, tokens);

        let main = mainline_of(&decoded);
        assert_eq!(main.len(), 4);
        assert!(matches!(main[3], Ply::Null));
        assert!(matches!(
            decode_game(&start, &bytes),
            Err(MoveBinError::MainlineNull)
        ));
    }

    #[test]
    fn consecutive_variations_branch_from_the_same_move() {
        let start = Board::default();
        let mut board = start.clone();
        let e4 = parse_san(&board, "e4").unwrap();
        board.play(e4);
        let c5 = parse_san(&board, "c5").unwrap();
        let e6 = parse_san(&board, "e6").unwrap();
        let e5 = parse_san(&board, "e5").unwrap();
        // 1. e4 e5 (1... c5) (1... e6)
        let tokens = vec![
            Token::Move(e4),
            Token::Move(e5),
            Token::VarStart,
            Token::Move(c5),
            Token::VarEnd,
            Token::VarStart,
            Token::Move(e6),
            Token::VarEnd,
        ];
        let bytes = encode_tokens(&start, &tokens).unwrap();
        assert_eq!(decode_tokens(&start, &bytes).unwrap(), tokens);
    }

    #[test]
    fn malformed_streams_error() {
        let start = Board::default();
        assert!(matches!(
            decode_tokens(&start, &[0]),
            Err(MoveBinError::MissingEnd)
        ));
        assert!(matches!(
            decode_tokens(&start, &[230, TOK_END]),
            Err(MoveBinError::ReservedByte(230))
        ));
        assert!(matches!(
            decode_tokens(&start, &[TOK_VAR_END, TOK_END]),
            Err(MoveBinError::Variation(_))
        ));
        // Unicode comment round-trip.
        let tokens = vec![Token::Comment("Zürich – Müller ♞".into())];
        let bytes = encode_tokens(&start, &tokens).unwrap();
        assert_eq!(decode_tokens(&start, &bytes).unwrap(), tokens);
    }

    #[test]
    fn v1_decoder_still_reads_legacy_blobs() {
        let (start, moves) = opera_moves();
        // Build a v1 blob by hand: bare indices.
        let mut board = start.clone();
        let mut v1 = Vec::new();
        for &mv in &moves {
            let idx = ordered_legal_moves(&board)
                .iter()
                .position(|&m| m == mv)
                .unwrap();
            v1.push(idx as u8);
            board.play(mv);
        }
        assert_eq!(decode_game_v1(&start, &v1).unwrap(), moves);
    }
}
