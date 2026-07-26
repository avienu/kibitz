//! PGN export of stored games: seven-tag roster, Elo/ECO tags, custom
//! start positions, and the full movetext token stream — moves, comments,
//! NAGs, nested variations and null moves (`--`).

use cozy_chess::{Board, Color};
use rusqlite::Connection;

use crate::movebin::{decode_tokens, Token};
use crate::san::format_san;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("no game with id {0}")]
    NoSuchGame(i64),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored movetext is corrupt: {0}")]
    Movetext(String),
}

fn tag(out: &mut String, name: &str, value: &str) {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    out.push_str(&format!("[{name} \"{escaped}\"]\n"));
}

/// Render one stored game as standard PGN.
pub fn export_pgn(conn: &Connection, game_id: i64) -> Result<String, ExportError> {
    let row = conn
        .query_row(
            "SELECT COALESCE(e.name,'?'), COALESCE(s.name,'?'), COALESCE(g.date,'????.??.??'),
                    COALESCE(g.round,'?'), COALESCE(wp.name,'?'), COALESCE(bp.name,'?'),
                    g.result, g.white_elo, g.black_elo, g.eco, g.start_fen, g.movetext
             FROM games g
             LEFT JOIN players wp ON wp.id = g.white_id
             LEFT JOIN players bp ON bp.id = g.black_id
             LEFT JOIN events e ON e.id = g.event_id
             LEFT JOIN sites s ON s.id = g.site_id
             WHERE g.id = ?1",
            [game_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Vec<u8>>(11)?,
                ))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ExportError::NoSuchGame(game_id),
            other => ExportError::Sqlite(other),
        })?;
    let (event, site, date, round, white, black, result, w_elo, b_elo, eco, start_fen, movetext) =
        row;
    let result_str = match result {
        1 => "1-0",
        2 => "0-1",
        3 => "1/2-1/2",
        _ => "*",
    };

    let mut out = String::new();
    tag(&mut out, "Event", &event);
    tag(&mut out, "Site", &site);
    tag(&mut out, "Date", &date);
    tag(&mut out, "Round", &round);
    tag(&mut out, "White", &white);
    tag(&mut out, "Black", &black);
    tag(&mut out, "Result", result_str);
    if let Some(e) = w_elo {
        tag(&mut out, "WhiteElo", &e.to_string());
    }
    if let Some(e) = b_elo {
        tag(&mut out, "BlackElo", &e.to_string());
    }
    if let Some(e) = &eco {
        tag(&mut out, "ECO", e);
    }
    let start: Board = match &start_fen {
        Some(fen) => {
            tag(&mut out, "SetUp", "1");
            tag(&mut out, "FEN", fen);
            fen.parse()
                .map_err(|e| ExportError::Movetext(format!("bad stored FEN: {e:?}")))?
        }
        None => Board::default(),
    };
    out.push('\n');

    let tokens =
        decode_tokens(&start, &movetext).map_err(|e| ExportError::Movetext(e.to_string()))?;
    // Generated narrations live beside the movetext; merge them in after
    // the mainline move (past its NAGs and any human comment).
    let narrations = crate::narrate::narrations(conn, game_id)
        .map_err(|e| ExportError::Movetext(e.to_string()))?;
    let tokens = merge_narrations(tokens, &narrations);
    let mut body = render_movetext(&start, &tokens);
    if !body.is_empty() {
        body.push(' ');
    }
    body.push_str(result_str);
    out.push_str(&wrap_80(&body));
    out.push('\n');
    Ok(out)
}

/// Splice generated narrations into the token stream: after the mainline
/// move at each narrated ply, past its NAG tokens and any human comment.
fn merge_narrations(
    tokens: Vec<Token>,
    narrations: &std::collections::HashMap<u32, String>,
) -> Vec<Token> {
    if narrations.is_empty() {
        return tokens;
    }
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len() + narrations.len());
    let mut depth = 0u32;
    let mut ply = 0u32;
    let mut pending: Option<&str> = None;
    for token in tokens {
        // A pending narration flushes before anything that isn't a NAG or
        // comment attached to the narrated move.
        if pending.is_some() && !matches!(token, Token::Nag(_) | Token::Comment(_)) {
            out.push(Token::Comment(pending.take().unwrap().to_string()));
        }
        match &token {
            Token::VarStart => depth += 1,
            Token::VarEnd => depth = depth.saturating_sub(1),
            Token::Move(_) | Token::Null if depth == 0 => {
                ply += 1;
                pending = narrations.get(&ply).map(String::as_str);
            }
            _ => {}
        }
        out.push(token);
    }
    if let Some(text) = pending {
        out.push(Token::Comment(text.to_string()));
    }
    out
}

/// Render the token stream as PGN movetext (no result, no wrapping).
fn render_movetext(start: &Board, tokens: &[Token]) -> String {
    /// One nesting level of the renderer.
    struct Level {
        board: Board,
        before: Option<(Board, u32)>, // board + fullmove before last move
        fullmove: u32,
        /// A move number must be printed even for Black's move (after a
        /// comment, variation, or at a variation/game start).
        force_number: bool,
    }
    let mut level = Level {
        board: start.clone(),
        before: None,
        // Exported fullmove numbering restarts at 1 (the FEN carries the
        // real counter for custom starts; SAN semantics are unaffected).
        fullmove: 1,
        force_number: true,
    };
    let mut stack: Vec<Level> = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    for token in tokens {
        match token {
            Token::Move(_) | Token::Null => {
                let white = level.board.side_to_move() == Color::White;
                let mut s = String::new();
                if white {
                    s.push_str(&format!("{}. ", level.fullmove));
                } else if level.force_number {
                    s.push_str(&format!("{}... ", level.fullmove));
                }
                match token {
                    Token::Move(mv) => s.push_str(&format_san(&level.board, *mv)),
                    _ => s.push_str("--"),
                }
                parts.push(s);
                level.before = Some((level.board.clone(), level.fullmove));
                match token {
                    Token::Move(mv) => level.board.play(*mv),
                    _ => {
                        level.board = level
                            .board
                            .null_move()
                            .expect("stored streams contain only legal nulls");
                    }
                }
                if !white {
                    level.fullmove += 1;
                }
                level.force_number = false;
            }
            Token::Nag(n) => parts.push(format!("${n}")),
            Token::Comment(text) => {
                // PGN brace comments cannot contain '}'.
                parts.push(format!("{{{}}}", text.replace('}', ")")));
                level.force_number = true;
            }
            Token::VarStart => {
                let (branch, branch_fullmove) = level
                    .before
                    .clone()
                    .expect("stored streams attach variations to a move");
                parts.push("(".to_string());
                stack.push(level);
                level = Level {
                    board: branch,
                    before: None,
                    fullmove: branch_fullmove,
                    force_number: true,
                };
            }
            Token::VarEnd => {
                parts.push(")".to_string());
                level = stack.pop().expect("balanced variations");
                level.force_number = true;
            }
        }
    }
    // Join, but glue "(" to the following token and ")" to the preceding.
    let mut body = String::new();
    for part in parts {
        if !body.is_empty() && part != ")" && !body.ends_with('(') {
            body.push(' ');
        }
        body.push_str(&part);
    }
    body
}

/// Soft-wrap at ~80 columns on spaces.
fn wrap_80(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 80);
    let mut line_len = 0usize;
    for word in text.split(' ') {
        if line_len > 0 && line_len + 1 + word.len() > 80 {
            out.push('\n');
            line_len = 0;
        } else if line_len > 0 {
            out.push(' ');
            line_len += 1;
        }
        out.push_str(word);
        line_len += word.len();
    }
    out
}
