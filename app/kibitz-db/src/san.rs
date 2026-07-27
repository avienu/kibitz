//! SAN (standard algebraic notation) parsing and formatting over cozy-chess.
//!
//! cozy-chess encodes castling as "king moves onto its own rook's square";
//! this module translates that to/from `O-O`/`O-O-O`.

use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SanError {
    #[error("empty SAN token")]
    Empty,
    #[error("malformed SAN token `{0}`")]
    Malformed(String),
    #[error("no legal move matches `{0}`")]
    NoMatch(String),
    #[error("SAN `{0}` is ambiguous")]
    Ambiguous(String),
}

fn piece_letter(p: Piece) -> Option<char> {
    match p {
        Piece::Knight => Some('N'),
        Piece::Bishop => Some('B'),
        Piece::Rook => Some('R'),
        Piece::Queen => Some('Q'),
        Piece::King => Some('K'),
        Piece::Pawn => None,
    }
}

fn letter_piece(c: char) -> Option<Piece> {
    match c {
        'N' => Some(Piece::Knight),
        'B' => Some(Piece::Bishop),
        'R' => Some(Piece::Rook),
        'Q' => Some(Piece::Queen),
        'K' => Some(Piece::King),
        _ => None,
    }
}

fn legal_moves(board: &Board) -> Vec<Move> {
    let mut out = Vec::with_capacity(64);
    board.generate_moves(|pm| {
        out.extend(pm);
        false
    });
    out
}

/// Is `mv` cozy-chess's king-onto-own-rook castling encoding?
fn is_castling(board: &Board, mv: Move) -> bool {
    board.piece_on(mv.from) == Some(Piece::King)
        && board.piece_on(mv.to) == Some(Piece::Rook)
        && board.color_on(mv.to) == board.color_on(mv.from)
}

/// Parse one SAN token in the context of `board`. Tolerant of decorations
/// (`+`, `#`, `!`, `?`, trailing `e.p.`) and of a missing/superfluous `x`.
pub fn parse_san(board: &Board, san: &str) -> Result<Move, SanError> {
    let mut s = san.trim();
    if s.is_empty() {
        return Err(SanError::Empty);
    }
    for suffix in ["e.p.", "ep"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped;
        }
    }
    let s = s.trim_end_matches(['+', '#', '!', '?']);
    if s.is_empty() {
        return Err(SanError::Malformed(san.to_string()));
    }

    // Castling.
    if matches!(s, "O-O" | "0-0" | "O-O-O" | "0-0-0") {
        let short = matches!(s, "O-O" | "0-0");
        let candidates: Vec<Move> = legal_moves(board)
            .into_iter()
            .filter(|&mv| is_castling(board, mv) && ((mv.to.file() > mv.from.file()) == short))
            .collect();
        return match candidates.as_slice() {
            [mv] => Ok(*mv),
            [] => Err(SanError::NoMatch(san.to_string())),
            _ => Err(SanError::Ambiguous(san.to_string())),
        };
    }

    // Promotion suffix: "e8=Q" (standard) or bare "e8Q".
    let (s, promotion) = {
        let bytes = s.as_bytes();
        let last = *bytes.last().unwrap() as char;
        if let Some(p) = letter_piece(last) {
            if p != Piece::King && bytes.len() >= 2 {
                let stem = &s[..s.len() - 1];
                let stem = stem.strip_suffix('=').unwrap_or(stem);
                (stem, Some(p))
            } else {
                (s, None)
            }
        } else {
            (s, None)
        }
    };

    let mut chars: Vec<char> = s.chars().collect();
    // Moving piece.
    let piece = if let Some(p) = chars.first().and_then(|&c| letter_piece(c)) {
        chars.remove(0);
        p
    } else {
        Piece::Pawn
    };
    // Destination square = trailing "<file><rank>".
    if chars.len() < 2 {
        return Err(SanError::Malformed(san.to_string()));
    }
    let rank_c = chars.pop().unwrap();
    let file_c = chars.pop().unwrap();
    let dest_file = File::try_index(file_c as usize - 'a' as usize)
        .ok_or_else(|| SanError::Malformed(san.to_string()))?;
    let dest_rank = Rank::try_index(rank_c as usize - '1' as usize)
        .ok_or_else(|| SanError::Malformed(san.to_string()))?;
    let dest = Square::new(dest_file, dest_rank);
    // Whatever remains is disambiguation (and possibly 'x').
    let mut from_file: Option<File> = None;
    let mut from_rank: Option<Rank> = None;
    for c in chars {
        match c {
            'x' => {}
            'a'..='h' => from_file = File::try_index(c as usize - 'a' as usize),
            '1'..='8' => from_rank = Rank::try_index(c as usize - '1' as usize),
            _ => return Err(SanError::Malformed(san.to_string())),
        }
    }

    let candidates: Vec<Move> = legal_moves(board)
        .into_iter()
        .filter(|&mv| {
            board.piece_on(mv.from) == Some(piece)
                && mv.to == dest
                && mv.promotion == promotion
                && from_file.is_none_or(|f| mv.from.file() == f)
                && from_rank.is_none_or(|r| mv.from.rank() == r)
                && !is_castling(board, mv)
        })
        .collect();
    match candidates.as_slice() {
        [mv] => Ok(*mv),
        [] => Err(SanError::NoMatch(san.to_string())),
        _ => Err(SanError::Ambiguous(san.to_string())),
    }
}

/// Format a legal move as SAN (with `+`/`#` suffix).
pub fn format_san(board: &Board, mv: Move) -> String {
    let mut out = String::new();
    let piece = board
        .piece_on(mv.from)
        .expect("move from an occupied square");

    if is_castling(board, mv) {
        out.push_str(if mv.to.file() > mv.from.file() {
            "O-O"
        } else {
            "O-O-O"
        });
    } else {
        let is_capture = board
            .color_on(mv.to)
            .is_some_and(|c| Some(c) != board.color_on(mv.from))
            || (piece == Piece::Pawn && mv.to.file() != mv.from.file());
        if let Some(l) = piece_letter(piece) {
            out.push(l);
            // Disambiguation among same-piece moves to the same square.
            let rivals: Vec<Move> = legal_moves(board)
                .into_iter()
                .filter(|&m| {
                    m.to == mv.to
                        && m.from != mv.from
                        && board.piece_on(m.from) == Some(piece)
                        && !is_castling(board, m)
                })
                .collect();
            if !rivals.is_empty() {
                let file_unique = rivals.iter().all(|m| m.from.file() != mv.from.file());
                let rank_unique = rivals.iter().all(|m| m.from.rank() != mv.from.rank());
                if file_unique {
                    out.push((b'a' + mv.from.file() as u8) as char);
                } else if rank_unique {
                    out.push((b'1' + mv.from.rank() as u8) as char);
                } else {
                    out.push((b'a' + mv.from.file() as u8) as char);
                    out.push((b'1' + mv.from.rank() as u8) as char);
                }
            }
            if is_capture {
                out.push('x');
            }
        } else {
            if is_capture {
                out.push((b'a' + mv.from.file() as u8) as char);
                out.push('x');
            }
        }
        out.push((b'a' + mv.to.file() as u8) as char);
        out.push((b'1' + mv.to.rank() as u8) as char);
        if let Some(p) = mv.promotion {
            out.push('=');
            out.push(piece_letter(p).expect("promotion piece has a letter"));
        }
    }

    let mut after = board.clone();
    after.play_unchecked(mv);
    if !after.checkers().is_empty() {
        let any_reply = !legal_moves(&after).is_empty();
        out.push(if any_reply { '+' } else { '#' });
    }
    out
}

/// Convenience: current side to move (used by callers rendering move lists).
pub fn side_to_move(board: &Board) -> Color {
    board.side_to_move()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(fen: &str) -> Board {
        fen.parse().unwrap()
    }

    fn play(board: &mut Board, san: &str) {
        let mv = parse_san(board, san).unwrap_or_else(|e| panic!("{e}"));
        board.play(mv);
    }

    #[test]
    fn parses_the_opera_game_mainline() {
        // Morphy vs Duke Karl / Count Isouard, Paris 1858 (public domain).
        let sans = "e4 e5 Nf3 d6 d4 Bg4 dxe5 Bxf3 Qxf3 dxe5 Bc4 Nf6 Qb3 Qe7 \
                    Nc3 c6 Bg5 b5 Nxb5 cxb5 Bxb5+ Nbd7 O-O-O Rd8 Rxd7 Rxd7 \
                    Rd1 Qe6 Bxd7+ Nxd7 Qb8+ Nxb8 Rd8#";
        let mut b = Board::default();
        for san in sans.split_whitespace() {
            play(&mut b, san);
        }
        assert!(legal_moves(&b).is_empty(), "final position is checkmate");
    }

    #[test]
    fn castling_both_sides() {
        let mut b = board("r3k2r/pppqpppp/2n2n2/8/8/2N2N2/PPPQPPPP/R3K2R w KQkq - 0 1");
        play(&mut b, "O-O");
        assert_eq!(b.king(Color::White), Square::G1);
        play(&mut b, "O-O-O");
        assert_eq!(b.king(Color::Black), Square::C8);
    }

    #[test]
    fn promotion_and_underpromotion() {
        let b = board("8/2P5/8/8/8/1k6/8/1K6 w - - 0 1");
        let q = parse_san(&b, "c8=Q").unwrap();
        assert_eq!(q.promotion, Some(Piece::Queen));
        let n = parse_san(&b, "c8N").unwrap();
        assert_eq!(n.promotion, Some(Piece::Knight));
    }

    #[test]
    fn file_rank_and_square_disambiguation() {
        // Two knights can reach d2; file disambiguation.
        let b = board("4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1");
        let mv = parse_san(&b, "Nbd2").unwrap();
        assert_eq!(mv.from, Square::B1);
        let mv = parse_san(&b, "Nfd2").unwrap();
        assert_eq!(mv.from, Square::F3);
        assert_eq!(parse_san(&b, "Nd2"), Err(SanError::Ambiguous("Nd2".into())));
    }

    #[test]
    fn en_passant_capture() {
        let mut b = board("4k3/8/8/8/4p3/8/3P4/4K3 w - - 0 1");
        play(&mut b, "d4");
        let mv = parse_san(&b, "exd3").unwrap();
        b.play(mv);
        assert_eq!(b.piece_on(Square::D3), Some(Piece::Pawn));
        assert_eq!(b.piece_on(Square::D4), None, "captured pawn removed");
    }

    #[test]
    fn format_round_trips_through_parse() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2P5/8/8/8/1k6/8/1K6 w - - 0 1",
            "r3k2r/pppqpppp/2n2n2/8/8/2N2N2/PPPQPPPP/R3K2R b KQkq - 0 1",
        ];
        for fen in fens {
            let b = board(fen);
            for mv in legal_moves(&b) {
                let san = format_san(&b, mv);
                let parsed = parse_san(&b, &san)
                    .unwrap_or_else(|e| panic!("round-trip failed for {san} on {fen}: {e}"));
                assert_eq!(parsed, mv, "san {san} on {fen}");
            }
        }
    }
}
