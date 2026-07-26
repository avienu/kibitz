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
}

/// Open (creating if necessary) a silman database and bring it up to the
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
    let applied: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?;
    for &(version, sql) in MIGRATIONS {
        if version > applied {
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
    if enc_v != ENCODING_VERSION as u32 {
        return Err(DbError::EncodingVersionMismatch {
            found: enc_v,
            expected: ENCODING_VERSION as u32,
        });
    }
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
