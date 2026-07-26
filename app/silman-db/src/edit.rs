//! Annotation editing: read a stored game as its token stream, apply a
//! modified stream back, and keep every derived structure (position index,
//! ply count, duplicate signature) consistent.

use cozy_chess::Board;
use rusqlite::{params, Connection};

use crate::import::PreparedGame;
use crate::movebin::{decode_tokens, Token};

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("no game with id {0}")]
    NoSuchGame(i64),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("movetext error: {0}")]
    Movetext(String),
}

/// Load a game's start position and full token stream.
pub fn game_tokens(conn: &Connection, game_id: i64) -> Result<(Board, Vec<Token>), EditError> {
    let (movetext, start_fen): (Vec<u8>, Option<String>) = conn
        .query_row(
            "SELECT movetext, start_fen FROM games WHERE id = ?1",
            [game_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => EditError::NoSuchGame(game_id),
            other => EditError::Sqlite(other),
        })?;
    let start: Board = match start_fen.as_deref() {
        Some(fen) => fen
            .parse()
            .map_err(|e| EditError::Movetext(format!("bad stored FEN: {e:?}")))?,
        None => Board::default(),
    };
    let tokens =
        decode_tokens(&start, &movetext).map_err(|e| EditError::Movetext(e.to_string()))?;
    Ok((start, tokens))
}

/// Persist an edited token stream: re-encode, update the game row, and
/// rebuild the game's slice of the position index (the mainline may have
/// changed). The header signature is untouched — edits do not change who
/// played when — but the move-sequence hash tracks the new mainline.
pub fn update_game_tokens(
    conn: &Connection,
    game_id: i64,
    tokens: &[Token],
) -> Result<(), EditError> {
    let (start, _) = game_tokens(conn, game_id)?;
    let built = PreparedGame::build(&start, tokens).map_err(EditError::Movetext)?;

    conn.execute_batch("BEGIN")?;
    conn.execute(
        "UPDATE games SET movetext = ?1, ply_count = ?2, moves_hash = ?3 WHERE id = ?4",
        params![
            built.movetext,
            built.ply_count as i64,
            built.moves_hash as i64,
            game_id
        ],
    )?;
    conn.execute("DELETE FROM positions WHERE game_id = ?1", [game_id])?;
    {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO positions (position_hash, game_id, ply, next_byte)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (ply, &h) in built.position_hashes.iter().enumerate() {
            stmt.execute(params![
                h as i64,
                game_id,
                ply as i64,
                built.next_indices[ply].map(|b| b as i64)
            ])?;
        }
    }
    conn.execute_batch("COMMIT")?;
    Ok(())
}
