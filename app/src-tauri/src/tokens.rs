//! JSON-facing annotation-token layer (Phase 2 annotation editing).
//!
//! IPC commands `get_game_tokens` / `update_game_tokens` expose a game's
//! full movetext token stream (silman_db::movebin::Token) as a tagged JSON
//! list the frontend can render and transform:
//!
//! ```json
//! [{"t":"move","san":"e4"}, {"t":"nag","value":1},
//!  {"t":"comment","text":"..."}, {"t":"varStart"}, {"t":"varEnd"},
//!  {"t":"null"}]
//! ```
//!
//! Moves cross the boundary as SAN, so both directions replay the stream
//! with the same board-stack semantics as movebin's encode/decode: a
//! variation branches from the position BEFORE the last move at the current
//! nesting level.

use cozy_chess::Board;
use serde::{Deserialize, Serialize};
use silman_db::movebin::Token;
use silman_db::san::{format_san, parse_san};
use tauri::State;

use crate::browse::{with_conn, DbState};

/// One movetext token in its JSON wire form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum JsonToken {
    Move { san: String },
    Nag { value: u8 },
    Comment { text: String },
    VarStart,
    VarEnd,
    Null,
}

/// Replay state: current board plus the board before the last move at this
/// nesting level (the branch point for variations) — mirrors movebin.
#[derive(Clone)]
struct Level {
    cur: Board,
    before_last: Option<Board>,
}

/// Serialize a decoded token stream to its JSON form (moves become SAN).
pub fn tokens_to_json(start: &Board, tokens: &[Token]) -> Result<Vec<JsonToken>, String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut level = Level {
        cur: start.clone(),
        before_last: None,
    };
    let mut stack: Vec<Level> = Vec::new();
    for token in tokens {
        match token {
            Token::Move(mv) => {
                out.push(JsonToken::Move {
                    san: format_san(&level.cur, *mv),
                });
                level.before_last = Some(level.cur.clone());
                level.cur.play(*mv);
            }
            Token::Null => {
                let next = level
                    .cur
                    .null_move()
                    .ok_or_else(|| "null move while in check".to_string())?;
                out.push(JsonToken::Null);
                level.before_last = Some(level.cur.clone());
                level.cur = next;
            }
            Token::Nag(value) => out.push(JsonToken::Nag { value: *value }),
            Token::Comment(text) => out.push(JsonToken::Comment { text: text.clone() }),
            Token::VarStart => {
                let branch = level
                    .before_last
                    .clone()
                    .ok_or_else(|| "variation before any move".to_string())?;
                out.push(JsonToken::VarStart);
                stack.push(level.clone());
                level = Level {
                    cur: branch,
                    before_last: None,
                };
            }
            Token::VarEnd => {
                out.push(JsonToken::VarEnd);
                level = stack
                    .pop()
                    .ok_or_else(|| "varEnd without varStart".to_string())?;
            }
        }
    }
    if !stack.is_empty() {
        return Err("unclosed variation".to_string());
    }
    Ok(out)
}

/// Parse the JSON form back into movebin tokens (SAN becomes moves, checked
/// for legality against the replayed position).
pub fn json_to_tokens(start: &Board, json: &[JsonToken]) -> Result<Vec<Token>, String> {
    let mut out = Vec::with_capacity(json.len());
    let mut level = Level {
        cur: start.clone(),
        before_last: None,
    };
    let mut stack: Vec<Level> = Vec::new();
    for (i, token) in json.iter().enumerate() {
        match token {
            JsonToken::Move { san } => {
                let mv = parse_san(&level.cur, san)
                    .map_err(|e| format!("token {i}: bad SAN {san:?}: {e}"))?;
                out.push(Token::Move(mv));
                level.before_last = Some(level.cur.clone());
                level.cur.play(mv);
            }
            JsonToken::Null => {
                let next = level
                    .cur
                    .null_move()
                    .ok_or_else(|| format!("token {i}: null move while in check"))?;
                out.push(Token::Null);
                level.before_last = Some(level.cur.clone());
                level.cur = next;
            }
            JsonToken::Nag { value } => out.push(Token::Nag(*value)),
            JsonToken::Comment { text } => out.push(Token::Comment(text.clone())),
            JsonToken::VarStart => {
                let branch = level
                    .before_last
                    .clone()
                    .ok_or_else(|| format!("token {i}: variation before any move"))?;
                out.push(Token::VarStart);
                stack.push(level.clone());
                level = Level {
                    cur: branch,
                    before_last: None,
                };
            }
            JsonToken::VarEnd => {
                out.push(Token::VarEnd);
                level = stack
                    .pop()
                    .ok_or_else(|| format!("token {i}: varEnd without varStart"))?;
            }
        }
    }
    if !stack.is_empty() {
        return Err("unclosed variation".to_string());
    }
    Ok(out)
}

/// `get_game_tokens` payload: the game's start position and token stream.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameTokens {
    /// FEN of the start position (standard start included, for convenience).
    pub start_fen: String,
    pub tokens: Vec<JsonToken>,
}

pub(crate) fn get_game_tokens_impl(
    conn: &rusqlite::Connection,
    game_id: i64,
) -> Result<GameTokens, String> {
    let (start, tokens) = silman_db::edit::game_tokens(conn, game_id).map_err(|e| e.to_string())?;
    Ok(GameTokens {
        start_fen: start.to_string(),
        tokens: tokens_to_json(&start, &tokens)?,
    })
}

pub(crate) fn update_game_tokens_impl(
    conn: &rusqlite::Connection,
    game_id: i64,
    tokens: &[JsonToken],
) -> Result<(), String> {
    let (start, _) = silman_db::edit::game_tokens(conn, game_id).map_err(|e| e.to_string())?;
    let decoded = json_to_tokens(&start, tokens)?;
    silman_db::edit::update_game_tokens(conn, game_id, &decoded).map_err(|e| e.to_string())
}

/// Full annotation token stream of one game, as JSON tokens.
#[tauri::command]
pub async fn get_game_tokens(
    state: State<'_, DbState>,
    game_id: i64,
) -> Result<GameTokens, String> {
    with_conn(&state, |conn| get_game_tokens_impl(conn, game_id))
}

/// Persist an edited token stream (re-encodes and rebuilds derived indexes).
#[tauri::command]
pub async fn update_game_tokens(
    state: State<'_, DbState>,
    game_id: i64,
    tokens: Vec<JsonToken>,
) -> Result<(), String> {
    with_conn(&state, |conn| {
        update_game_tokens_impl(conn, game_id, &tokens)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use silman_db::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// Annotated fixture: comment, NAG, nested variations — imported
    /// through the real silman-db PGN importer so the movetext blob is
    /// exactly what production games carry.
    const ANNOTATED: &str = r#"[White "A"]
[Black "B"]
[Result "*"]

1. e4 $1 {best by test} e5 (1... c5 $2 2. Nf3 (2. c3)) 2. Nf3 Nc6 *
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "test".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(ANNOTATED)).unwrap();
        assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);
        (dir, conn)
    }

    #[test]
    fn token_json_round_trips_through_the_serde_layer() {
        let (_dir, conn) = fixture_db();
        let gt = get_game_tokens_impl(&conn, 1).unwrap();
        assert!(gt.start_fen.starts_with("rnbqkbnr/pppppppp/"));

        // Wire shape is the documented tagged form.
        let json = serde_json::to_string(&gt.tokens).unwrap();
        for needle in [
            r#"{"t":"move","san":"e4"}"#,
            r#"{"t":"nag","value":1}"#,
            r#"{"t":"comment","text":"best by test"}"#,
            r#"{"t":"varStart"}"#,
            r#"{"t":"varEnd"}"#,
            r#"{"t":"move","san":"c5"}"#,
        ] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }
        // The variation's c5 was rendered from the branch position (after
        // 1.e4, NOT after 1...e5): "c5" is only legal there.
        let reparsed: Vec<JsonToken> = serde_json::from_str(&json).unwrap();
        assert_eq!(reparsed, gt.tokens);

        // JSON -> movebin tokens equals the stored stream exactly.
        let (start, stored) = silman_db::edit::game_tokens(&conn, 1).unwrap();
        let back = json_to_tokens(&start, &gt.tokens).unwrap();
        assert_eq!(back, stored);

        // Persisting the unchanged JSON is a no-op round trip.
        update_game_tokens_impl(&conn, 1, &gt.tokens).unwrap();
        let gt2 = get_game_tokens_impl(&conn, 1).unwrap();
        assert_eq!(gt2.tokens, gt.tokens);
    }

    #[test]
    fn frontend_style_edits_persist_and_export() {
        let (_dir, conn) = fixture_db();
        let gt = get_game_tokens_impl(&conn, 1).unwrap();

        // Simulate the UI transforms: add a variation (2... d6) after the
        // mainline move 4 (Nc6), and a comment on 2. Nf3.
        let mut tokens = gt.tokens.clone();
        tokens.push(JsonToken::VarStart);
        tokens.push(JsonToken::Move { san: "d6".into() });
        tokens.push(JsonToken::VarEnd);
        let nf3_pos = tokens
            .iter()
            .position(|t| matches!(t, JsonToken::Move { san } if san == "Nf3"))
            .unwrap();
        // First "Nf3" in the stream is the variation's; find the mainline
        // one (the second).
        let nf3_main = tokens
            .iter()
            .enumerate()
            .skip(nf3_pos + 1)
            .find_map(|(i, t)| matches!(t, JsonToken::Move { san } if san == "Nf3").then_some(i))
            .unwrap();
        tokens.insert(
            nf3_main + 1,
            JsonToken::Comment {
                text: "develops".into(),
            },
        );

        update_game_tokens_impl(&conn, 1, &tokens).unwrap();
        let pgn = silman_db::export::export_pgn(&conn, 1).unwrap();
        assert!(pgn.contains("2. Nf3 {develops}"), "{pgn}");
        assert!(pgn.contains("Nc6 (2... d6)"), "{pgn}");
        // Prior annotations still intact.
        assert!(pgn.contains("1. e4 $1 {best by test}"), "{pgn}");
    }

    #[test]
    fn bad_streams_are_rejected_with_clear_errors() {
        let (_dir, conn) = fixture_db();
        let err = update_game_tokens_impl(&conn, 1, &[JsonToken::Move { san: "Qxg8".into() }])
            .unwrap_err();
        assert!(err.contains("Qxg8"), "{err}");

        let err = update_game_tokens_impl(&conn, 1, &[JsonToken::VarStart]).unwrap_err();
        assert!(err.contains("variation before any move"), "{err}");

        let err = update_game_tokens_impl(
            &conn,
            1,
            &[
                JsonToken::Move { san: "e4".into() },
                JsonToken::VarStart,
                JsonToken::Move { san: "d4".into() },
            ],
        )
        .unwrap_err();
        assert!(err.contains("unclosed variation"), "{err}");
    }
}
