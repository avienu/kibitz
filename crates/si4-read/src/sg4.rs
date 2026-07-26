//! .sg4 game record decoding (moves, markers, comments).
//!
//! Written from docs/SI4_FORMAT_NOTES.md §3, with the documented gaps
//! resolved EMPIRICALLY against the maintainer's own real SCID databases
//! (allowed by the cleanroom rule: spec documentation + our own test
//! files; no SCID source consulted). Empirical findings, each validated by
//! whole-corpus decoding (every move legal, mainline ply count equal to
//! the index's, final material equal to the index signature):
//!
//! 1. Non-standard tags: a first byte >= 0xF1 is a single-byte *tag code*
//!    for a common tag (observed 0xF3 = Annotator), followed by
//!    {value_len, value}; bytes 0x01..=0xF0 are a literal tag-name length
//!    as documented. 0x00 ends the tag list.
//! 2. Pawn piece numbers are 8..=15 for files a..h; pawn move code 15 is
//!    the (undocumented) two-square advance.
//! 3. The community doc's ROOK table is transposed: rook code 0-7 is
//!    to-FILE on the same rank, 8-15 is to-RANK+8 on the same file —
//!    which makes rook and queen encodings mutually consistent. The queen
//!    table is correct as documented (bit3=1 = to-rank, bit3=0 = to-file,
//!    same-file signalling a diagonal whose destination is the next byte
//!    minus 64).
//! 4. A move byte of 0x00 (king, code 0) is a null move ("pass").
//! 5. There is no start-of-game marker; marker 13 introduces a variation
//!    that branches from the position *before* the preceding move.
//! 6. Piece numbers are indices into a per-side SWAP-REMOVE piece list:
//!    initial order as documented (K, Ra, Nb, Bc, Q, Bf, Ng, Rh, pawns
//!    a..h), and when a piece is captured the last list element is moved
//!    into its slot. Numbers are therefore reused mid-game.

use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square};

#[derive(Debug, thiserror::Error)]
pub enum Sg4Error {
    #[error("record truncated while reading {0}")]
    Truncated(&'static str),
    #[error("ply {ply}: piece number {piece_num} is not on the board")]
    DeadPiece { ply: usize, piece_num: u8 },
    #[error("ply {ply}: {msg}")]
    BadMove { ply: usize, msg: String },
    #[error("bad start FEN {0:?}")]
    BadFen(String),
    #[error("variation end without matching start")]
    UnbalancedVariation,
    #[error("null move in a position where it is illegal (in check)")]
    NullInCheck,
}

/// One decoded movetext token, in traversal order, variations included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameToken {
    Move(Move),
    Null,
    Nag(u8),
    /// Index into [`DecodedGame::comments`].
    Comment(usize),
    VarStart,
    VarEnd,
}

/// Fully decoded game record: the complete token stream plus mainline
/// convenience fields.
#[derive(Debug, Default)]
pub struct DecodedGame {
    /// None = standard start position.
    pub start_fen: Option<String>,
    /// Full token stream (moves at every depth, NAGs, comment refs,
    /// variation markers).
    pub tokens: Vec<GameToken>,
    /// Mainline moves only (nulls appear as placeholders; see null_plies).
    pub moves: Vec<Move>,
    /// Plies within the mainline that are null moves ("--"). The move at
    /// that index in `moves` is a placeholder and must not be played.
    pub null_plies: Vec<usize>,
    pub nag_count: u32,
    pub comment_count: u32,
    pub variation_count: u32,
    /// Trailing comment texts, traversal order, Latin-1-decoded.
    pub comments: Vec<String>,
    pub tags: Vec<(String, String)>,
}

/// A side's swap-remove piece list (empirical finding 6): piece numbers in
/// the move stream index into this list; captures move the last element
/// into the freed slot.
type SideTracker = Vec<(Square, Piece)>;

#[derive(Clone)]
struct State {
    board: Board,
    trackers: [SideTracker; 2], // [White, Black]
}

fn tracker_mut(state: &mut State, color: Color) -> &mut SideTracker {
    &mut state.trackers[color as usize]
}

/// Swap-remove the piece on `victim_sq` from `list`, if present.
fn capture_at(list: &mut SideTracker, victim_sq: Square) {
    if let Some(idx) = list.iter().position(|(s, _)| *s == victim_sq) {
        list.swap_remove(idx);
    }
}

/// Standard-start piece numbering (docs §3.2): 0=K 1=Ra 2=Nb 3=Bc 4=Q
/// 5=Bf 6=Ng 7=Rh, pawns 8..=15 = files a..h (empirical finding 2).
fn standard_tracker(color: Color) -> SideTracker {
    let back = match color {
        Color::White => Rank::First,
        Color::Black => Rank::Eighth,
    };
    let pawn_rank = match color {
        Color::White => Rank::Second,
        Color::Black => Rank::Seventh,
    };
    let mut t = Vec::with_capacity(16);
    t.push((Square::new(File::E, back), Piece::King));
    t.push((Square::new(File::A, back), Piece::Rook));
    t.push((Square::new(File::B, back), Piece::Knight));
    t.push((Square::new(File::C, back), Piece::Bishop));
    t.push((Square::new(File::D, back), Piece::Queen));
    t.push((Square::new(File::F, back), Piece::Bishop));
    t.push((Square::new(File::G, back), Piece::Knight));
    t.push((Square::new(File::H, back), Piece::Rook));
    for f in File::ALL {
        t.push((Square::new(f, pawn_rank), Piece::Pawn));
    }
    t
}

/// Custom-start numbering (docs §3.2): each side's pieces in FEN scan order
/// (rank 8 → rank 1, file a → h within a rank), king swapped to number 0.
fn fen_order_tracker(board: &Board, color: Color) -> SideTracker {
    let mut t: SideTracker = Vec::with_capacity(16);
    for rank_idx in (0..8).rev() {
        for file_idx in 0..8 {
            let sq = Square::new(File::index(file_idx), Rank::index(rank_idx));
            if board.color_on(sq) == Some(color) && t.len() < 16 {
                t.push((sq, board.piece_on(sq).expect("occupied")));
            }
        }
    }
    // Swap the king to number 0.
    if let Some(king_idx) = t.iter().position(|(_, p)| *p == Piece::King) {
        t.swap(0, king_idx);
    }
    t
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn u8(&mut self, what: &'static str) -> Result<u8, Sg4Error> {
        let b = *self.bytes.get(self.pos).ok_or(Sg4Error::Truncated(what))?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Sg4Error> {
        let end = self.pos + n;
        if end > self.bytes.len() {
            return Err(Sg4Error::Truncated(what));
        }
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

const KNIGHT_DELTAS: [i8; 8] = [-17, -15, -10, -6, 6, 10, 15, 17];
const KING_DELTAS: [i8; 8] = [-9, -8, -7, -1, 1, 7, 8, 9];

fn offset_square(
    sq: Square,
    delta: i8,
    what: &'static str,
    ply: usize,
) -> Result<Square, Sg4Error> {
    let idx = sq as i16 + delta as i16;
    if !(0..64).contains(&idx) {
        return Err(Sg4Error::BadMove {
            ply,
            msg: format!("{what} delta {delta} leaves the board from {sq}"),
        });
    }
    Ok(Square::index(idx as usize))
}

/// Compute the destination square (and promotion) encoded by `code` for the
/// piece currently of type `piece` on `from`. Queen diagonal moves read
/// their destination from the cursor.
fn decode_dest(
    cur: &mut Cursor,
    piece: Piece,
    from: Square,
    code: u8,
    stm: Color,
    ply: usize,
) -> Result<(Square, Option<Piece>), Sg4Error> {
    let bad = |msg: String| Sg4Error::BadMove { ply, msg };
    match piece {
        Piece::King => {
            // Codes 1-8 = deltas; 9 = O-O-O (-2); 10 = O-O (+2).
            let delta = match code {
                1..=8 => KING_DELTAS[code as usize - 1],
                9 => -2,
                10 => 2,
                _ => return Err(bad(format!("king move code {code}"))),
            };
            Ok((offset_square(from, delta, "king", ply)?, None))
        }
        Piece::Knight => {
            let delta = *KNIGHT_DELTAS
                .get(code.wrapping_sub(1) as usize)
                .ok_or_else(|| bad(format!("knight move code {code}")))?;
            Ok((offset_square(from, delta, "knight", ply)?, None))
        }
        Piece::Rook => {
            // Empirical finding 3: the community doc's rook table is
            // transposed — codes 0-7 are to-FILE (same rank), 8-15 are
            // to-RANK (same file), consistent with the queen encoding.
            let dest = if code < 8 {
                Square::new(File::index(code as usize), from.rank())
            } else {
                Square::new(from.file(), Rank::index(code as usize - 8))
            };
            Ok((dest, None))
        }
        Piece::Bishop => {
            let dest_file = File::index((code & 0x7) as usize);
            let f0 = from.file() as i16;
            let r0 = from.rank() as i16;
            let rank = if code & 0x8 == 0 {
                // a1-h8-direction diagonal: rank - file is constant.
                r0 - f0 + dest_file as i16
            } else {
                // h1-a8-direction diagonal: rank + file is constant.
                r0 + f0 - dest_file as i16
            };
            if !(0..8).contains(&rank) {
                return Err(bad(format!("bishop code {code} leaves the board")));
            }
            Ok((Square::new(dest_file, Rank::index(rank as usize)), None))
        }
        Piece::Queen => {
            if code & 0x8 != 0 {
                Ok((
                    Square::new(from.file(), Rank::index((code & 0x7) as usize)),
                    None,
                ))
            } else {
                let dest_file = File::index((code & 0x7) as usize);
                if dest_file != from.file() {
                    Ok((Square::new(dest_file, from.rank()), None))
                } else {
                    // Same-file "rank move to own square" = diagonal escape:
                    // destination is the next byte minus 64.
                    let b = cur.u8("queen diagonal destination")?;
                    if !(64..128).contains(&b) {
                        return Err(bad(format!("queen diagonal byte {b:#x} out of range")));
                    }
                    Ok((Square::index((b - 64) as usize), None))
                }
            }
        }
        Piece::Pawn => {
            let sign: i8 = if stm == Color::White { 1 } else { -1 };
            if code == 15 {
                // Empirical finding 2: two-square advance.
                return Ok((offset_square(from, 16 * sign, "pawn", ply)?, None));
            }
            let delta = match code % 3 {
                0 => 7,
                1 => 8,
                _ => 9,
            } * sign;
            let promotion = match code {
                0..=2 => None,
                3..=5 => Some(Piece::Queen),
                6..=8 => Some(Piece::Rook),
                9..=11 => Some(Piece::Bishop),
                12..=14 => Some(Piece::Knight),
                _ => return Err(bad(format!("pawn move code {code}"))),
            };
            Ok((offset_square(from, delta, "pawn", ply)?, promotion))
        }
    }
}

/// Apply one decoded si4 move to `state`, translating castling into
/// cozy-chess's king-onto-rook form, and return the cozy move played.
fn apply_move(
    state: &mut State,
    piece_num: u8,
    code: u8,
    cur: &mut Cursor,
    ply: usize,
) -> Result<Move, Sg4Error> {
    let stm = state.board.side_to_move();
    let (from, piece) = *tracker_mut(state, stm)
        .get(piece_num as usize)
        .ok_or(Sg4Error::DeadPiece { ply, piece_num })?;
    let (dest, promotion) = decode_dest(cur, piece, from, code, stm, ply)?;

    // Castling: king moves two files; cozy encodes it as king-onto-own-rook.
    let castle = piece == Piece::King && (code == 9 || code == 10);
    let mv = if castle {
        let rook_file = if code == 10 { File::H } else { File::A };
        let rook_sq = Square::new(rook_file, from.rank());
        Move {
            from,
            to: rook_sq,
            promotion: None,
        }
    } else {
        Move {
            from,
            to: dest,
            promotion,
        }
    };

    // Legality check + play.
    if !state.board.is_legal(mv) {
        return Err(Sg4Error::BadMove {
            ply,
            msg: format!("decoded move {mv} is illegal (piece {piece:?} #{piece_num} code {code})"),
        });
    }

    // Track captures (including en passant).
    let opponent = !stm;
    let mut victim_sq = dest;
    if piece == Piece::Pawn && dest.file() != from.file() && state.board.piece_on(dest).is_none() {
        victim_sq = Square::new(dest.file(), from.rank()); // en passant
    }
    if state.board.color_on(victim_sq) == Some(opponent) {
        capture_at(tracker_mut(state, opponent), victim_sq);
    }

    // Update the mover (and rook, for castling).
    {
        let own = tracker_mut(state, stm);
        if castle {
            let (kf, rf) = if code == 10 {
                (File::G, File::F)
            } else {
                (File::C, File::D)
            };
            own[piece_num as usize] = (Square::new(kf, from.rank()), Piece::King);
            let rook_start = Square::new(if code == 10 { File::H } else { File::A }, from.rank());
            if let Some(idx) = own.iter().position(|(s, _)| *s == rook_start) {
                own[idx] = (Square::new(rf, from.rank()), Piece::Rook);
            }
        } else {
            own[piece_num as usize] = (dest, promotion.unwrap_or(piece));
        }
    }

    state.board.play_unchecked(mv);
    Ok(mv)
}

/// Decode one .sg4 game record (bytes `offset..offset+length` of the file).
pub fn decode_game(record: &[u8]) -> Result<DecodedGame, Sg4Error> {
    let mut cur = Cursor {
        bytes: record,
        pos: 0,
    };
    let mut out = DecodedGame::default();

    // Non-standard tags (empirical finding 1).
    loop {
        let b = cur.u8("tag list")?;
        if b == 0 {
            break;
        }
        if b >= 0xF1 {
            let vlen = cur.u8("coded tag value length")? as usize;
            let value = latin1(cur.take(vlen, "coded tag value")?);
            out.tags.push((format!("scid-tag-{b:#04x}"), value));
        } else {
            let name = latin1(cur.take(b as usize, "tag name")?);
            let vlen = cur.u8("tag value length")? as usize;
            let value = latin1(cur.take(vlen, "tag value")?);
            out.tags.push((name, value));
        }
    }

    let flags = cur.u8("record flags")?;
    let board = if flags & 0x1 != 0 {
        // Custom start: NUL-terminated FEN follows.
        let start = cur.pos;
        while cur.u8("start FEN")? != 0 {}
        let fen = latin1(&cur.bytes[start..cur.pos - 1]);
        // SCID FENs may omit move counters; pad for the parser.
        let full = if fen.split_ascii_whitespace().count() == 4 {
            format!("{fen} 0 1")
        } else {
            fen.clone()
        };
        out.start_fen = Some(full.clone());
        full.parse::<Board>()
            .map_err(|_| Sg4Error::BadFen(fen.clone()))?
    } else {
        Board::default()
    };

    let mut state = State {
        trackers: if out.start_fen.is_none() {
            [
                standard_tracker(Color::White),
                standard_tracker(Color::Black),
            ]
        } else {
            [
                fen_order_tracker(&board, Color::White),
                fen_order_tracker(&board, Color::Black),
            ]
        },
        board,
    };

    // Variation stack: (state to restore at variation end, branch point at
    // push time — so consecutive variations on the same move both rewind).
    #[allow(clippy::type_complexity)]
    let mut stack: Vec<(State, Option<State>)> = Vec::new();
    let mut before_last: Option<State> = None;
    let mut depth = 0u32;

    loop {
        let b = cur.u8("move stream")?;
        match b {
            11 => {
                let nag = cur.u8("NAG value")?;
                out.nag_count += 1;
                out.tokens.push(GameToken::Nag(nag));
            }
            12 => {
                out.tokens
                    .push(GameToken::Comment(out.comment_count as usize));
                out.comment_count += 1;
            }
            13 => {
                // Variation branching from before the previous move
                // (empirical finding 5).
                out.variation_count += 1;
                depth += 1;
                out.tokens.push(GameToken::VarStart);
                stack.push((state.clone(), before_last.clone()));
                if let Some(prev) = &before_last {
                    state = prev.clone();
                }
            }
            14 => {
                depth = depth.checked_sub(1).ok_or(Sg4Error::UnbalancedVariation)?;
                out.tokens.push(GameToken::VarEnd);
                (state, before_last) = stack.pop().ok_or(Sg4Error::UnbalancedVariation)?;
            }
            15 => break,
            0 => {
                // Null move (empirical finding 4).
                before_last = Some(state.clone());
                let next = state.board.null_move().ok_or(Sg4Error::NullInCheck)?;
                state.board = next;
                out.tokens.push(GameToken::Null);
                if depth == 0 {
                    out.null_plies.push(out.moves.len());
                    out.moves.push(Move {
                        from: Square::A1,
                        to: Square::A1,
                        promotion: None,
                    });
                }
            }
            _ => {
                let ply = out.moves.len() + 1;
                before_last = Some(state.clone());
                let mv = apply_move(&mut state, b >> 4, b & 0xF, &mut cur, ply)?;
                out.tokens.push(GameToken::Move(mv));
                if depth == 0 {
                    out.moves.push(mv);
                }
            }
        }
    }

    // Trailing comment texts.
    while cur.pos < cur.bytes.len() {
        let start = cur.pos;
        while cur.pos < cur.bytes.len() && cur.bytes[cur.pos] != 0 {
            cur.pos += 1;
        }
        out.comments.push(latin1(&cur.bytes[start..cur.pos]));
        cur.pos += 1; // NUL
        if out.comments.len() > out.comment_count as usize + 4 {
            break; // padding garbage; stop defensively
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mainline_with_variation_nag_comment() {
        // 1.e4 e5 (1...c5) 2.Nf3 Nc6 3.Bc4 Nf6 4.O-O $1 {ok}
        // Bytes derived by hand from the documented + empirical encoding.
        let record: &[u8] = &[
            0x00, // no tags
            0x00, // flags: standard start
            0xcf, // e-pawn (12) double push  -> e4
            0xcf, // black e-pawn double push -> e5
            13, 0xaf, 14,   // variation before e5: 1...c5
            0x67, // Ng1 code 7 (+15)         -> Nf3
            0x22, // Nb8 code 2 (-15)         -> Nc6
            0x5a, // Bf1, anti-diagonal, file c -> Bc4
            0x61, // Ng8 code 1 (-17)         -> Nf6
            0x0a, // king code 10             -> O-O
            11, 1,  // NAG $1
            12, // comment marker
            15, // end of game
            b'o', b'k', 0,
        ];
        let g = decode_game(record).unwrap();
        assert_eq!(g.moves.len(), 7);
        assert_eq!(g.variation_count, 1);
        assert_eq!(g.nag_count, 1);
        assert_eq!(g.comment_count, 1);
        assert_eq!(g.comments, vec!["ok".to_string()]);
        // Replay: all legal, ends castled.
        let mut board = Board::default();
        for mv in &g.moves {
            board.play(*mv);
        }
        assert_eq!(board.king(Color::White), Square::G1);
    }

    #[test]
    fn custom_start_promotion_and_queen_diagonal() {
        // FEN 8/2P5/8/8/8/1k6/8/1K6 w: FEN-order numbering with king
        // swapped to 0 gives white 0=Kb1, 1=Pc7.
        let mut record: Vec<u8> = vec![0x00, 0x01];
        record.extend_from_slice(b"8/2P5/8/8/8/1k6/8/1K6 w - - 0 1\0");
        record.extend_from_slice(&[
            0x14, // pawn (1) code 4: push + promote Q -> c8=Q
            0x06, // black king code 6 (+7) -> Ka4
            0x12, 0x68, // queen (still piece 1): same-file escape byte, dest 0x68-64=40=a6
            15,
        ]);
        let g = decode_game(&record).unwrap();
        assert!(g.start_fen.is_some());
        assert_eq!(g.moves.len(), 3);
        assert_eq!(g.moves[0].promotion, Some(Piece::Queen));
        assert_eq!(g.moves[2].to, Square::A6);
    }

    #[test]
    fn coded_nonstandard_tag_and_empty_game() {
        // Observed in real data: 0xF3 = Annotator, single-byte tag code.
        let record: &[u8] = &[0xf3, 2, b'L', b'C', 0x00, 0x00, 15];
        let g = decode_game(record).unwrap();
        assert_eq!(g.tags.len(), 1);
        assert_eq!(g.tags[0].1, "LC");
        assert!(g.moves.is_empty());
    }

    #[test]
    fn null_move_is_tracked() {
        let record: &[u8] = &[0x00, 0x00, 0xcf, 0x00, 0x67, 15];
        let g = decode_game(record).unwrap();
        assert_eq!(g.moves.len(), 3);
        assert_eq!(g.null_plies, vec![1]);
    }
}
