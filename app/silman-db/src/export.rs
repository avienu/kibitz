//! PGN export of stored games.
//!
//! Exports everything the database currently stores: seven-tag roster,
//! Elo/ECO tags, custom start positions, and the mainline. Comments, NAGs
//! and variations are not yet stored (DECISIONS_NEEDED.md items 1–2), so
//! round-trip equality holds for the representable subset.

use cozy_chess::Board;
use rusqlite::Connection;

use crate::movebin::decode_game;
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

    let moves = decode_game(&start, &movetext).map_err(|e| ExportError::Movetext(e.to_string()))?;
    let mut board = start.clone();
    let mut line_len = 0usize;
    let mut body = String::new();
    let start_fullmove_white = board.side_to_move() == cozy_chess::Color::White;
    // Exported fullmove numbering restarts at 1 (the FEN carries the real
    // counter for custom starts; SAN semantics are unaffected).
    let mut fullmove = 1u32;
    let mut first = true;
    for mv in moves {
        let white_to_move = board.side_to_move() == cozy_chess::Color::White;
        let mut token = String::new();
        if white_to_move {
            token.push_str(&format!("{fullmove}. "));
        } else if first && !start_fullmove_white {
            token.push_str(&format!("{fullmove}... "));
        }
        token.push_str(&format_san(&board, mv));
        if !white_to_move {
            fullmove += 1;
        }
        if line_len + token.len() + 1 > 80 {
            body.push('\n');
            line_len = 0;
        } else if !body.is_empty() {
            body.push(' ');
            line_len += 1;
        }
        line_len += token.len();
        body.push_str(&token);
        board.play(mv);
        first = false;
    }
    if !body.is_empty() {
        body.push(' ');
    }
    body.push_str(result_str);
    out.push_str(&body);
    out.push('\n');
    Ok(out)
}
