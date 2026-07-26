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

/// One continuation in the opening tree for a position.
#[derive(Debug)]
pub struct TreeMove {
    pub san: String,
    pub count: i64,
    pub white_wins: i64,
    pub draws: i64,
    pub black_wins: i64,
    /// Average rating of the side playing the move (games with a rating).
    pub avg_elo: Option<i64>,
    /// Linear performance estimate: avg opponent rating + 800·score − 400.
    pub perf: Option<i64>,
}

/// Aggregate the opening tree for `fen`: every move played from this
/// position across the database, with counts, W/D/L from White's
/// perspective, average mover rating and performance.
pub fn opening_tree(conn: &Connection, fen: &str) -> Result<(Vec<TreeMove>, Duration), QueryError> {
    let board: Board = fen.parse().map_err(QueryError::Fen)?;
    let hash = crate::hash::position_hash(&board) as i64;
    let mover_is_white = board.side_to_move() == cozy_chess::Color::White;
    let (mover_elo, opp_elo) = if mover_is_white {
        ("g.white_elo", "g.black_elo")
    } else {
        ("g.black_elo", "g.white_elo")
    };

    let start = Instant::now();
    let sql = format!(
        "SELECT p.next_byte,
                COUNT(*),
                SUM(g.result = 1), SUM(g.result = 3), SUM(g.result = 2),
                AVG({mover_elo}), AVG({opp_elo})
         FROM positions p
         JOIN games g ON g.id = p.game_id
         WHERE p.position_hash = ?1 AND p.next_byte IS NOT NULL
         GROUP BY p.next_byte
         ORDER BY COUNT(*) DESC"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let ordered = crate::movebin::ordered_legal_moves(&board);
    let rows = stmt
        .query_map([hash], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, Option<f64>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (byte, count, ww, dd, bw, avg_mover, avg_opp) in rows {
        let san = ordered
            .get(byte as usize)
            .map(|&mv| crate::san::format_san(&board, mv))
            .unwrap_or_else(|| format!("<byte {byte}>"));
        let mover_wins = if mover_is_white { ww } else { bw };
        let score = (mover_wins as f64 + dd as f64 / 2.0) / count.max(1) as f64;
        let perf = avg_opp.map(|o| (o + 800.0 * score - 400.0).round() as i64);
        out.push(TreeMove {
            san,
            count,
            white_wins: ww,
            draws: dd,
            black_wins: bw,
            avg_elo: avg_mover.map(|e| e.round() as i64),
            perf,
        });
    }
    let elapsed = start.elapsed();
    Ok((out, elapsed))
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
