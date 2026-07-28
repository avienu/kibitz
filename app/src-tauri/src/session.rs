//! Session restore (maintainer request, run 10): the app reopens where it
//! was closed. Two storage layers with different owners:
//!
//! - The LAST DATABASE PATH cannot live inside any database (you have to
//!   know it before opening one), so it goes in a tiny JSON file in the
//!   app config dir. Written on every successful `open_database`.
//! - Everything else (active screen, database-screen filters, last game +
//!   ply + orientation) lives in the open database's `meta` table so it
//!   travels with the database file: `ui_session` here, `last_game`
//!   maintained by home.rs.
//!
//! Deep links (`#db=`, `#game=`) override restore in the frontend and are
//! never persisted.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::browse::{with_conn, DbState};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
struct SessionFile {
    /// Absolute path of the last successfully opened database.
    last_db: String,
}

fn session_file(dir: &Path) -> PathBuf {
    dir.join("session.json")
}

/// Record `db_path` as the last-opened database. Errors are reported (the
/// caller logs and moves on — failing to remember must never fail an open).
pub(crate) fn remember_db_path(dir: &Path, db_path: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("config dir: {e}"))?;
    let json = serde_json::to_string_pretty(&SessionFile {
        last_db: db_path.to_string(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(session_file(dir), json).map_err(|e| format!("session file: {e}"))
}

/// The remembered database path, if any (missing/corrupt file → None —
/// restore is best-effort by design).
pub(crate) fn recall_db_path(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(session_file(dir)).ok()?;
    let parsed: SessionFile = serde_json::from_slice(&bytes).ok()?;
    Some(parsed.last_db)
}

/// Config dir for this app (wrapped so commands share one resolution).
fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("no app config dir: {e}"))
}

/// Called by `open_database` after a successful open.
pub(crate) fn remember_db(app: &tauri::AppHandle, db_path: &str) {
    if let Ok(dir) = config_dir(app) {
        if let Err(e) = remember_db_path(&dir, db_path) {
            eprintln!("session: could not remember database path: {e}");
        }
    }
}

/// The database to reopen at launch, if one was remembered.
#[tauri::command]
pub async fn last_database(app: tauri::AppHandle) -> Result<Option<String>, String> {
    Ok(config_dir(&app).ok().and_then(|d| recall_db_path(&d)))
}

/// Opaque UI-session blob (frontend-owned schema: active screen +
/// database-screen filter state). Stored in the meta table of the OPEN
/// database so it travels with the file.
#[tauri::command]
pub async fn ui_session_get(state: State<'_, DbState>) -> Result<Option<String>, String> {
    with_conn(&state, |conn| {
        conn.query_row("SELECT value FROM meta WHERE key = 'ui_session'", [], |r| {
            r.get::<_, String>(0)
        })
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    })
}

#[tauri::command]
pub async fn ui_session_set(state: State<'_, DbState>, json: String) -> Result<(), String> {
    with_conn(&state, |conn| {
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('ui_session', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [&json],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_and_recall_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("cfg"); // create_dir_all must handle depth
        remember_db_path(&nested, "/tmp/a.sqlite").unwrap();
        assert_eq!(recall_db_path(&nested).as_deref(), Some("/tmp/a.sqlite"));
        // Overwrite wins.
        remember_db_path(&nested, "/tmp/b.sqlite").unwrap();
        assert_eq!(recall_db_path(&nested).as_deref(), Some("/tmp/b.sqlite"));
    }

    #[test]
    fn recall_is_none_on_missing_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(recall_db_path(dir.path()), None);
        std::fs::write(session_file(dir.path()), b"not json").unwrap();
        assert_eq!(recall_db_path(dir.path()), None);
    }

    #[test]
    fn ui_session_round_trips_in_meta() {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        // No row yet.
        let none: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key='ui_session'", [], |r| {
                r.get(0)
            })
            .ok();
        assert!(none.is_none());
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('ui_session', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [r#"{"view":"database"}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('ui_session', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [r#"{"view":"game"}"#],
        )
        .unwrap();
        let v: String = conn
            .query_row("SELECT value FROM meta WHERE key='ui_session'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, r#"{"view":"game"}"#);
    }
}
