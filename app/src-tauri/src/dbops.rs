//! Run-4 UI wiring commands over silman-db: per-game analyses for the
//! legacy-vs-fresh eval display (verdict 3c), annotate / re-analyze / job
//! runner (goal 2 + verdict 3d), annotated-PGN export (goal 2), player
//! profile (goal 4), and window-title cosmetics (verdict 4).
//!
//! Product principle (CLAUDE.md #6): the engine runs ONLY inside the
//! user-initiated `run_jobs` worker thread, through the job queue. Nothing
//! else here spawns an engine — the engine identity shown in the status
//! strip comes from the last completed job's stored result JSON.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};

/// True while a `run_jobs` worker thread is executing.
#[derive(Default)]
pub struct JobsWorker(pub Arc<AtomicBool>);

// ---------------------------------------------------------------------------
// game_analyses (verdict 3c)
// ---------------------------------------------------------------------------

/// One stored engine evaluation of a game position. `eval_cp` POV differs
/// by kind: 'fresh' rows are side-to-move POV at that ply (White-POV =
/// negate when ply is odd); 'legacy-import' rows are already White-POV
/// (SCID convention). The frontend does the display conversion.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRow {
    pub ply: i64,
    pub kind: String,
    pub engine: String,
    pub depth: Option<i64>,
    pub nodes: Option<i64>,
    pub eval_cp: i64,
    pub created_at: String,
}

pub(crate) fn game_analyses_impl(
    conn: &Connection,
    game_id: i64,
) -> Result<Vec<AnalysisRow>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT ply, kind, engine, depth, nodes, eval_cp, created_at
             FROM analyses WHERE game_id = ?1
             ORDER BY ply, kind = 'fresh' DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;
    stmt.query_map([game_id], |r| {
        Ok(AnalysisRow {
            ply: r.get(0)?,
            kind: r.get(1)?,
            engine: r.get(2)?,
            depth: r.get(3)?,
            nodes: r.get(4)?,
            eval_cp: r.get(5)?,
            created_at: r.get(6)?,
        })
    })
    .and_then(|it| it.collect::<Result<Vec<_>, _>>())
    .map_err(|e| e.to_string())
}

/// All stored evaluations for one game, ordered ply ascending with fresh
/// rows before legacy imports at the same ply (newest first within a kind).
#[tauri::command]
pub async fn game_analyses(
    state: State<'_, DbState>,
    game_id: i64,
) -> Result<Vec<AnalysisRow>, String> {
    with_conn(&state, |conn| game_analyses_impl(conn, game_id))
}

// ---------------------------------------------------------------------------
// annotate / re-analyze / run jobs (goal 2 + verdict 3d)
// ---------------------------------------------------------------------------

/// Confirm-job and re-analysis node budget for UI-triggered runs.
const UI_NODES: u64 = 200_000;
/// Inline-comment cap for one UI-triggered annotate pass.
const UI_MAX_COMMENTS: u32 = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotateSummary {
    pub positions_analyzed: u32,
    pub screens_fired: u32,
    pub jobs_enqueued: u32,
    pub comments_added: u32,
}

/// Statically annotate one game (no engine) and enqueue bounded
/// confirmation jobs for fired WSUI screens.
#[tauri::command]
pub async fn annotate_game(
    state: State<'_, DbState>,
    game_id: i64,
) -> Result<AnnotateSummary, String> {
    with_conn(&state, |conn| {
        let r = silman_db::annotate::annotate_game(conn, game_id, UI_NODES, UI_MAX_COMMENTS)
            .map_err(|e| e.to_string())?;
        Ok(AnnotateSummary {
            positions_analyzed: r.positions_analyzed,
            screens_fired: r.screens_fired,
            jobs_enqueued: r.jobs_enqueued,
            comments_added: r.comments_added,
        })
    })
}

/// Enqueue one bounded eval per mainline position of the game (verdict
/// 3d). Returns the number of jobs enqueued; nothing runs until the user
/// starts the job runner.
#[tauri::command]
pub async fn reanalyze_game(state: State<'_, DbState>, game_id: i64) -> Result<u32, String> {
    with_conn(&state, |conn| {
        silman_db::jobs::enqueue_reanalyze(conn, game_id, UI_NODES).map_err(|e| e.to_string())
    })
}

fn run_jobs_worker(db_path: &Path, engine_path: &Path) -> Result<(), String> {
    // A dedicated connection: the UI connection stays free for polling.
    let conn = silman_db::db::open(db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    silman_db::jobs::reset_running(&conn).map_err(|e| e.to_string())?;
    silman_db::jobs::run_pending(&conn, engine_path, u32::MAX).map_err(|e| e.to_string())?;
    silman_db::annotate::fold_back(&conn).map_err(|e| e.to_string())?;
    Ok(())
}

/// Start a background worker draining the job queue (user-initiated, the
/// only engine entry point here), then fold completed verdicts back into
/// the stored annotations. Progress is polled via `jobs_status`.
#[tauri::command]
pub async fn run_jobs(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
) -> Result<(), String> {
    let db_path: String = with_conn(&state, |conn| {
        conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    })?;
    let engine_path = silman_db::engine::resolve_engine_path().ok_or_else(|| {
        "no engine found (set SILMAN_STOCKFISH, add tools/stockfish, or put stockfish on PATH)"
            .to_string()
    })?;
    if worker.0.swap(true, Ordering::SeqCst) {
        return Err("a job run is already in progress".to_string());
    }
    let flag = Arc::clone(&worker.0);
    std::thread::spawn(move || {
        if let Err(e) = run_jobs_worker(Path::new(&db_path), &engine_path) {
            eprintln!("silman job worker failed: {e}");
        }
        flag.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobsStatus {
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
    /// True while a `run_jobs` worker thread is alive.
    pub worker_active: bool,
    /// Engine identity from the most recently completed job's stored
    /// result JSON (no engine is spawned to obtain this).
    pub engine: Option<String>,
}

pub(crate) fn jobs_status_impl(
    conn: &Connection,
    worker_active: bool,
) -> Result<JobsStatus, String> {
    let (pending, running, done, failed) =
        silman_db::jobs::counts(conn).map_err(|e| e.to_string())?;
    let last_result: Option<String> = conn
        .query_row(
            "SELECT result FROM jobs
             WHERE status = 'done' AND result IS NOT NULL
             ORDER BY updated_at DESC, id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let engine = last_result
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["engine"].as_str().map(str::to_string));
    Ok(JobsStatus {
        pending,
        running,
        done,
        failed,
        worker_active,
        engine,
    })
}

/// Job-queue counts + last-run engine identity, for the UI status strip.
#[tauri::command]
pub async fn jobs_status(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
) -> Result<JobsStatus, String> {
    let active = worker.0.load(Ordering::SeqCst);
    with_conn(&state, |conn| jobs_status_impl(conn, active))
}

// ---------------------------------------------------------------------------
// export (goal 2), profile (goal 4), window title (verdict 4)
// ---------------------------------------------------------------------------

/// Render one stored game (with all annotations) as standard PGN text.
#[tauri::command]
pub async fn export_game_pgn(state: State<'_, DbState>, game_id: i64) -> Result<String, String> {
    with_conn(&state, |conn| {
        silman_db::export::export_pgn(conn, game_id).map_err(|e| e.to_string())
    })
}

/// Number of most-recent games a profile build may scan.
const PROFILE_MAX_GAMES: u32 = 2000;

/// Build the full player profile (static analysis + stored evals; no
/// engine is spawned).
#[tauri::command]
pub async fn build_profile(
    state: State<'_, DbState>,
    player: String,
) -> Result<silman_profile::PlayerProfile, String> {
    with_conn(&state, |conn| {
        silman_db::profile::build_profile(conn, &player, PROFILE_MAX_GAMES)
            .map_err(|e| e.to_string())
    })
}

/// Set the native window title (used as "silman — <db filename>" once a
/// database is open).
#[tauri::command]
pub fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use silman_db::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// Opera game (public domain, 33 plies): long enough for the profile
    /// pipeline's 10-move minimum and known to be a Morphy win.
    const FIXTURE: &str = r#"[Event "Casual Game"]
[Site "Paris FRA"]
[Date "1858.11.02"]
[White "Morphy, Paul"]
[Black "Duke Karl / Count Isouard"]
[Result "1-0"]

1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7
8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7
14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);
        (dir, conn)
    }

    fn plant(conn: &Connection, game_id: i64, ply: i64, kind: &str, engine: &str, cp: i64) {
        conn.execute(
            "INSERT INTO analyses (game_id, ply, kind, engine, eval_cp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![game_id, ply, kind, engine, cp],
        )
        .unwrap();
    }

    #[test]
    fn game_analyses_orders_by_ply_with_fresh_first() {
        let (_dir, conn) = fixture_db();
        // Planted deliberately out of display order.
        plant(&conn, 1, 3, "legacy-import", "Rybka 4 x64", 50);
        plant(&conn, 1, 3, "fresh", "Stockfish 17", -40);
        plant(&conn, 1, 1, "legacy-import", "Rybka 4 x64", 30);
        // A second fresh at ply 3: newest (highest id) must come first.
        plant(&conn, 1, 3, "fresh", "Stockfish 17.1", -35);

        let rows = game_analyses_impl(&conn, 1).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!((rows[0].ply, rows[0].kind.as_str()), (1, "legacy-import"));
        assert_eq!(rows[0].eval_cp, 30);
        assert!(!rows[0].created_at.is_empty());
        // Ply 3: fresh before legacy, newest fresh first.
        assert_eq!((rows[1].ply, rows[1].kind.as_str()), (3, "fresh"));
        assert_eq!(rows[1].engine, "Stockfish 17.1");
        assert_eq!(rows[1].eval_cp, -35);
        assert_eq!((rows[2].ply, rows[2].kind.as_str()), (3, "fresh"));
        assert_eq!(rows[2].engine, "Stockfish 17");
        assert_eq!((rows[3].ply, rows[3].kind.as_str()), (3, "legacy-import"));

        // Untouched game id -> empty, not an error.
        assert!(game_analyses_impl(&conn, 999).unwrap().is_empty());
    }

    /// POV convention (verdict 3c): fresh rows are side-to-move POV at
    /// their ply, legacy rows are already White-POV. Verified end to end
    /// through the profile pipeline's conversion/defense counters, which
    /// consume subject-POV (here: White = Morphy) evals.
    #[test]
    fn eval_pov_fresh_is_stm_legacy_is_white() {
        let (_dir, conn) = fixture_db();
        // Ply 5 is odd -> Black to move -> fresh -250 means +2.50 for
        // White. If fresh rows were (wrongly) read as White-POV this would
        // register as a lost position instead of a winning one.
        plant(&conn, 1, 5, "fresh", "Stockfish 17", -250);
        // Legacy rows carry White-POV directly: -150 is worse for White.
        // If legacy rows were (wrongly) negated at odd plies this would
        // read +1.50 and never trip the defense counter.
        plant(&conn, 1, 9, "legacy-import", "Rybka 4 x64", -150);

        let p = silman_db::profile::build_profile(&conn, "Morphy, Paul", 100).unwrap();
        assert_eq!(p.games, 1);
        assert_eq!(
            p.conversion.winning_reached, 1,
            "fresh -250 at odd ply is White +2.50"
        );
        assert_eq!(p.conversion.converted_wins, 1, "Morphy won");
        assert_eq!(
            p.conversion.losing_reached, 1,
            "legacy -150 stays White -1.50"
        );
        assert_eq!(p.conversion.held, 1, "the game was not lost");
    }

    #[test]
    fn jobs_status_reports_counts_and_last_run_engine() {
        let (_dir, conn) = fixture_db();
        let s = jobs_status_impl(&conn, false).unwrap();
        assert_eq!((s.pending, s.running, s.done, s.failed), (0, 0, 0, 0));
        assert_eq!(s.engine, None);
        assert!(!s.worker_active);

        // An older completed job and a newer one: the newer engine wins.
        conn.execute(
            "INSERT INTO jobs (purpose, payload, status, result, updated_at)
             VALUES ('reanalyze', '{}', 'done',
                     '{\"engine\":\"Old Engine 1\",\"score_cp\":0}',
                     '2020-01-01 00:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (purpose, payload, status, result)
             VALUES ('reanalyze', '{}', 'done',
                     '{\"engine\":\"Stockfish 17\",\"score_cp\":12}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs (purpose, payload) VALUES ('reanalyze', '{}')",
            [],
        )
        .unwrap();

        let s = jobs_status_impl(&conn, true).unwrap();
        assert_eq!((s.pending, s.running, s.done, s.failed), (1, 0, 2, 0));
        assert_eq!(s.engine.as_deref(), Some("Stockfish 17"));
        assert!(s.worker_active);
    }
}
