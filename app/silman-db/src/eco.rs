//! ECO classification via the bundled CC0 lichess-org/chess-openings
//! dataset (data/openings/*.tsv; see docs/LICENSES.md).
//!
//! On first use the dataset is replayed into the `openings` table: one row
//! per (line, ply) position hash. A game's ECO is the code of the deepest
//! opening position its mainline reaches (transposition-aware by
//! construction, since matching is by position hash, not move order).

use cozy_chess::Board;
use rusqlite::{params, Connection};

use crate::hash::position_hash;
use crate::san::parse_san;

const TSVS: &[&str] = &[
    include_str!("../../../data/openings/a.tsv"),
    include_str!("../../../data/openings/b.tsv"),
    include_str!("../../../data/openings/c.tsv"),
    include_str!("../../../data/openings/d.tsv"),
    include_str!("../../../data/openings/e.tsv"),
];

/// Populate the `openings` table from the bundled dataset if it is empty.
/// Returns the number of position rows present afterwards.
pub fn ensure_openings(conn: &Connection) -> anyhow::Result<i64> {
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM openings", [], |r| r.get(0))?;
    if existing > 0 {
        return Ok(existing);
    }
    conn.execute_batch("BEGIN")?;
    {
        let mut stmt = conn.prepare(
            "INSERT INTO openings (position_hash, eco, name, ply) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for tsv in TSVS {
            for line in tsv.lines().skip(1) {
                let mut cols = line.split('\t');
                let (Some(eco), Some(name), Some(pgn)) = (cols.next(), cols.next(), cols.next())
                else {
                    continue;
                };
                let mut board = Board::default();
                let mut ply = 0i64;
                for token in pgn.split_ascii_whitespace() {
                    if token.ends_with('.') {
                        continue; // move number
                    }
                    let Ok(mv) = parse_san(&board, token) else {
                        // Dataset lines are curated; a parse failure means a
                        // token form we don't handle — skip the rest of the
                        // line rather than store a wrong position.
                        break;
                    };
                    board.play(mv);
                    ply += 1;
                    stmt.execute(params![position_hash(&board) as i64, eco, name, ply])?;
                }
            }
        }
    }
    conn.execute_batch("COMMIT")?;
    Ok(conn.query_row("SELECT COUNT(*) FROM openings", [], |r| r.get(0))?)
}

/// The ECO code and opening name of the deepest opening-book position in
/// `hashes` (a game's per-ply position hashes, in order).
pub fn classify(conn: &Connection, hashes: &[u64]) -> rusqlite::Result<Option<(String, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT eco, name FROM openings WHERE position_hash = ?1 ORDER BY ply DESC LIMIT 1",
    )?;
    // Openings are at most ~35 plies deep; scan the game's prefix from the
    // deepest ply backwards and return the first hit.
    for &h in hashes.iter().take(40).rev() {
        let hit = stmt
            .query_row([h as i64], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(Some)
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;
        if hit.is_some() {
            return Ok(hit);
        }
    }
    Ok(None)
}
