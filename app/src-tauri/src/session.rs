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

/// Rewrite a remembered database path that still points inside `from`, so
/// it names the same file under `to`. Best-effort: no session file, an
/// unreadable one, or a path that lives elsewhere all leave it untouched.
pub(crate) fn repoint_remembered_db(config_dir: &Path, from: &Path, to: &Path) {
    let Some(old) = recall_db_path(config_dir) else {
        return;
    };
    let Ok(rest) = Path::new(&old).strip_prefix(from) else {
        return;
    };
    let moved = to.join(rest);
    if let Err(e) = remember_db_path(config_dir, &moved.to_string_lossy()) {
        eprintln!("session: could not repoint the remembered database: {e}");
    }
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

// ---------------------------------------------------------------------------
// Database relocation (maintainer request, 2026-07-29): the dev-era
// default lives inside a Dropbox-synced folder; at 1.2 GB+ the cloud
// client re-hashes every write, saturating the machine, and cloud sync
// interfering with a live SQLite file risks corruption. One click moves
// the database to the app's own storage.
// ---------------------------------------------------------------------------

/// Copy the open database to `dest` as a single consistent snapshot.
/// `VACUUM INTO` takes a read transaction, so it needs no exclusive lock
/// — but the CALLER must ensure no background writer (sync, jobs) is
/// active, or writes landing after the snapshot would be silently left
/// behind in the old file.
pub(crate) fn snapshot_db_to(conn: &rusqlite::Connection, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        return Err(format!(
            "{} already exists — remove or rename it first (refusing to overwrite a database)",
            dest.display()
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])
        .map_err(|e| format!("snapshot failed: {e}"))?;
    Ok(())
}

/// True when either background worker could be writing the database.
pub(crate) fn workers_busy(net_active: bool, jobs_active: bool) -> bool {
    net_active || jobs_active
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateReport {
    /// Where the database now lives (the app opened it already).
    pub new_path: String,
    /// The old file, left untouched as a backup.
    pub old_path: String,
}

/// Move the open database into the app's data dir: snapshot via
/// `VACUUM INTO` (also shedding WAL bloat), reopen from the new
/// location, remember it for the next launch. The old file stays as a
/// backup for the user to delete once satisfied.
#[tauri::command]
pub async fn migrate_database_to_app_storage(
    app: tauri::AppHandle,
    state: State<'_, DbState>,
    net: State<'_, crate::netops::NetWorker>,
    jobs: State<'_, crate::dbops::JobsWorker>,
) -> Result<MigrateReport, String> {
    use std::sync::atomic::Ordering;
    if workers_busy(
        net.active.load(Ordering::SeqCst),
        jobs.active.load(Ordering::SeqCst),
    ) {
        return Err(
            "a sync or analysis batch is running — wait for it to finish so the copy \
             cannot miss late writes"
                .to_string(),
        );
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let dest = data_dir.join("kibitz.sqlite");

    let mut guard = state
        .0
        .lock()
        .map_err(|_| "db state poisoned".to_string())?;
    let conn = guard.as_ref().ok_or("no database open")?;
    let old_path: String = conn
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if Path::new(&old_path).starts_with(&data_dir) {
        return Err("the database already lives in app storage".to_string());
    }
    snapshot_db_to(conn, &dest)?;

    // Swap the live connection to the new file (holding the lock so no
    // command can slip through against the old one mid-swap).
    let new_conn = kibitz_db::db::open(&dest).map_err(|e| e.to_string())?;
    new_conn
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    *guard = Some(new_conn);
    drop(guard);

    remember_db(&app, &dest.to_string_lossy());
    Ok(MigrateReport {
        new_path: dest.to_string_lossy().into_owned(),
        old_path,
    })
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
    fn snapshot_copies_a_consistent_database_and_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.sqlite");
        let conn = kibitz_db::db::open(&src).unwrap();
        conn.execute_batch(
            "INSERT INTO players (name) VALUES ('A'); INSERT INTO players (name) VALUES ('B');",
        )
        .unwrap();
        let dest = dir.path().join("nested/dir/kibitz.sqlite");
        snapshot_db_to(&conn, &dest).unwrap();
        let copy = kibitz_db::db::open(&dest).unwrap();
        let n: i64 = copy
            .query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        // Never overwrite an existing database.
        let err = snapshot_db_to(&conn, &dest).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn busy_workers_block_migration() {
        assert!(workers_busy(true, false));
        assert!(workers_busy(false, true));
        assert!(!workers_busy(false, false));
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
