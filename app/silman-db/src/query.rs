//! Position search: which games reached a given FEN.

use std::time::{Duration, Instant};

use cozy_chess::Board;
use rusqlite::Connection;

#[derive(Debug)]
pub struct GameHit {
    pub game_id: i64,
    pub white: String,
    pub black: String,
    pub event: String,
    pub date: String,
    pub result: &'static str,
    /// First ply at which the position occurred (1-based).
    pub ply: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    // cozy-chess's FenParseError does not implement std::error::Error.
    #[error("bad FEN: {0:?}")]
    Fen(cozy_chess::FenParseError),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn result_str(code: i64) -> &'static str {
    match code {
        1 => "1-0",
        2 => "0-1",
        3 => "1/2-1/2",
        _ => "*",
    }
}

/// Find all games that reached the position given by `fen` (position-hash
/// lookup; halfmove/fullmove counters are ignored by the hash). Returns the
/// hits and the pure query duration (excluding FEN parsing).
pub fn find_fen(conn: &Connection, fen: &str) -> Result<(Vec<GameHit>, Duration), QueryError> {
    let board: Board = fen.parse().map_err(QueryError::Fen)?;
    let hash = crate::hash::position_hash(&board) as i64;

    let start = Instant::now();
    let mut stmt = conn.prepare_cached(
        "SELECT g.id,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                COALESCE(e.name, '?'), COALESCE(g.date, '?'),
                g.result, MIN(p.ply)
         FROM positions p
         JOIN games g ON g.id = p.game_id
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         LEFT JOIN events e ON e.id = g.event_id
         WHERE p.position_hash = ?1
         GROUP BY g.id
         ORDER BY g.id",
    )?;
    let hits = stmt
        .query_map([hash], |row| {
            Ok(GameHit {
                game_id: row.get(0)?,
                white: row.get(1)?,
                black: row.get(2)?,
                event: row.get(3)?,
                date: row.get(4)?,
                result: result_str(row.get(5)?),
                ply: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let elapsed = start.elapsed();
    Ok((hits, elapsed))
}

/// Database summary counts for `silman-cli stats`.
#[derive(Debug)]
pub struct DbStats {
    pub games: i64,
    pub players: i64,
    pub positions: i64,
    pub sources: i64,
}

pub fn stats(conn: &Connection) -> rusqlite::Result<DbStats> {
    let one = |sql: &str| conn.query_row(sql, [], |r| r.get(0));
    Ok(DbStats {
        games: one("SELECT COUNT(*) FROM games")?,
        players: one("SELECT COUNT(*) FROM players")?,
        positions: one("SELECT COUNT(*) FROM positions")?,
        sources: one("SELECT COUNT(*) FROM sources")?,
    })
}
