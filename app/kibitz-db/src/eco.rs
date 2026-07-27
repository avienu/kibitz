//! ECO classification via the bundled CC0 lichess-org/chess-openings
//! dataset (data/openings/*.tsv; see docs/LICENSES.md).
//!
//! On first use the dataset is replayed into the `openings` table: one row
//! per (line, ply) position hash. A game's ECO is the code of the deepest
//! opening position its mainline reaches (transposition-aware by
//! construction, since matching is by position hash, not move order).

use cozy_chess::Board;
use rusqlite::{params, Connection, OptionalExtension};

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
    // Openings are at most ~35 plies deep; scan the game's prefix from the
    // deepest ply backwards and return the first hit.
    for &h in hashes.iter().take(40).rev() {
        let hit = classify_hash(conn, h)?;
        if hit.is_some() {
            return Ok(hit);
        }
    }
    Ok(None)
}

/// The ECO code and opening name recorded for one book position hash
/// (deepest entry first; deterministic tiebreak). `None` when the position
/// is not in the bundled dataset.
pub fn classify_hash(conn: &Connection, hash: u64) -> rusqlite::Result<Option<(String, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT eco, name FROM openings WHERE position_hash = ?1
         ORDER BY ply DESC, eco, name LIMIT 1",
    )?;
    stmt.query_row([hash as i64], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
}

/// The canonical display name for an ECO code, from the bundled dataset.
/// Deterministic rule: the shortest name recorded under the exact code —
/// which is the bare-code line of the dataset (C41 → "Philidor Defense",
/// not one of its named sub-variations) — ties broken lexicographically.
/// `None` for unknown codes. Requires [`ensure_openings`] to have run
/// (every importer runs it; callers on foreign connections should too).
pub fn name_for(conn: &Connection, code: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT name FROM openings WHERE eco = ?1 ORDER BY LENGTH(name), name LIMIT 1",
    )?;
    stmt.query_row([code], |r| r.get(0)).optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_for_resolves_known_codes_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        ensure_openings(&conn).unwrap();

        let philidor = name_for(&conn, "C41").unwrap().expect("C41 is known");
        assert!(
            philidor.contains("Philidor"),
            "C41 must resolve to a Philidor name, got {philidor:?}"
        );
        // The bare-code entry, not a sub-variation (shortest wins).
        assert!(
            !philidor.contains(':'),
            "expected the base name, got {philidor:?}"
        );
        // Deterministic: repeated calls agree.
        assert_eq!(name_for(&conn, "C41").unwrap().as_deref(), Some(&*philidor));
        assert_eq!(
            name_for(&conn, "B20").unwrap().as_deref(),
            Some("Sicilian Defense")
        );

        // Unknown codes resolve to None, not an error.
        assert_eq!(name_for(&conn, "Z99").unwrap(), None);
        assert_eq!(name_for(&conn, "").unwrap(), None);
    }

    #[test]
    fn classify_hash_names_book_positions() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        ensure_openings(&conn).unwrap();

        // After 1. e4 c5 the position is the Sicilian Defense (B20).
        let mut board = Board::default();
        for san in ["e4", "c5"] {
            let mv = parse_san(&board, san).unwrap();
            board.play(mv);
        }
        let (eco, name) = classify_hash(&conn, position_hash(&board))
            .unwrap()
            .expect("1.e4 c5 is in book");
        assert_eq!(eco, "B20");
        assert!(name.contains("Sicilian"), "got {name:?}");

        // A random non-book hash yields None.
        assert_eq!(classify_hash(&conn, 0xDEAD_BEEF).unwrap(), None);
    }
}
