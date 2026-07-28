//! SQLite connection management and numbered migrations.

use rusqlite::Connection;
use std::path::Path;

use crate::movebin::ENCODING_VERSION;

/// Version of the position-hash function used by the `positions` index.
/// Version 1 = `crate::hash::position_hash` (ep-normalized
/// `cozy_chess::Board::hash()`, cozy-chess 0.3.x). If the hash function ever
/// changes (library upgrade or normalization change), bump this and rebuild
/// the index; `open` refuses databases whose recorded version differs.
pub const POSITION_HASH_VERSION: u32 = 1;

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_init.sql")),
    (2, include_str!("../migrations/0002_openings_tree_twic.sql")),
    (3, include_str!("../migrations/0003_start_fen.sql")),
    (
        4,
        include_str!("../migrations/0004_source_kind_duplicates.sql"),
    ),
    (5, include_str!("../migrations/0005_jobs.sql")),
    (6, include_str!("../migrations/0006_analyses.sql")),
    (7, include_str!("../migrations/0007_narrations.sql")),
    (8, include_str!("../migrations/0008_repertoire.sql")),
    (9, include_str!("../migrations/0009_tactics.sql")),
    // 10 is reserved by parallel work; `migrate` applies per-version (not by
    // MAX), so 0010 can land after 0011 without being skipped.
    (11, include_str!("../migrations/0011_endgames.sql")),
    (12, include_str!("../migrations/0012_aliases.sql")),
    (13, include_str!("../migrations/0013_book_extensions.sql")),
];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database was written with position_hash_version {found}, this build expects {expected}; rebuild the position index")]
    HashVersionMismatch { found: u32, expected: u32 },
    #[error(
        "database was written with move encoding_version {found}, this build expects {expected}"
    )]
    EncodingVersionMismatch { found: u32, expected: u32 },
    #[error("v1→v2 movetext upgrade failed on game {game_id}: {msg}")]
    UpgradeFailed { game_id: i64, msg: String },
}

/// Open (creating if necessary) a kibitz database and bring it up to the
/// current schema version.
pub fn open(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    check_versions(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )?;
    // Applied per version (not by MAX): the registered list may carry gaps
    // when a number is reserved by parallel work, and the reserved migration
    // must still run when it lands later.
    for &(version, sql) in MIGRATIONS {
        let done: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |r| r.get(0),
        )?;
        if !done {
            conn.execute_batch("BEGIN;")?;
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
            conn.execute_batch("COMMIT;")?;
        }
    }
    Ok(())
}

fn check_versions(conn: &Connection) -> Result<(), DbError> {
    let hash_v = get_or_init_meta(conn, "position_hash_version", POSITION_HASH_VERSION)?;
    if hash_v != POSITION_HASH_VERSION {
        return Err(DbError::HashVersionMismatch {
            found: hash_v,
            expected: POSITION_HASH_VERSION,
        });
    }
    let enc_v = get_or_init_meta(conn, "encoding_version", ENCODING_VERSION as u32)?;
    match enc_v {
        v if v == ENCODING_VERSION as u32 => Ok(()),
        1 => upgrade_encoding_v1_to_v2(conn),
        found => Err(DbError::EncodingVersionMismatch {
            found,
            expected: ENCODING_VERSION as u32,
        }),
    }
}

/// One-shot re-encode of every stored game from encoding v1 (bare move
/// indices) to v2 (token stream). Decided 2026-07-25: a single migration,
/// not lazy per-row versioning — carrying two live encodings indefinitely
/// is complexity with no payoff at this data size. The `positions` index
/// is unaffected (its next_byte column stores ordered-legal-move indices,
/// which are identical in both versions).
fn upgrade_encoding_v1_to_v2(conn: &Connection) -> Result<(), DbError> {
    use crate::movebin::{decode_game_v1, encode_game};
    use cozy_chess::Board;

    conn.execute_batch("BEGIN")?;
    let games: Vec<(i64, Vec<u8>, Option<String>)> = {
        let mut stmt =
            conn.prepare("SELECT id, movetext, start_fen FROM games WHERE encoding_version = 1")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut update =
        conn.prepare("UPDATE games SET movetext = ?1, encoding_version = 2 WHERE id = ?2")?;
    for (id, blob, start_fen) in games {
        let start: Board = match start_fen.as_deref() {
            Some(fen) => fen.parse().map_err(|e| DbError::UpgradeFailed {
                game_id: id,
                msg: format!("bad start FEN: {e:?}"),
            })?,
            None => Board::default(),
        };
        let moves = decode_game_v1(&start, &blob).map_err(|e| DbError::UpgradeFailed {
            game_id: id,
            msg: e.to_string(),
        })?;
        let v2 = encode_game(&start, &moves).expect("v1 games re-encode losslessly");
        update.execute(rusqlite::params![v2, id])?;
    }
    drop(update);
    conn.execute(
        "UPDATE meta SET value = '2' WHERE key = 'encoding_version'",
        [],
    )?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}

fn get_or_init_meta(conn: &Connection, key: &str, default: u32) -> Result<u32, DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, default.to_string()],
    )?;
    let v: String = conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))?;
    Ok(v.parse().unwrap_or(0))
}

/// FNV-1a 64-bit hash: stable across runs, platforms and Rust releases,
/// which std's hashers do not guarantee. Used for duplicate-detection
/// signatures (NOT for the position index).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
