//! Opening triage IPC (run 10): the ranked deviation/gap/frontier report,
//! book-extension requests, and the honest status the UI polls.
//!
//! Product principle (CLAUDE.md #6): the report is a static database walk
//! — no engine. A book extension is an EXPLICIT user click, which both
//! enqueues the job (json_extract-deduped, kibitz_db::jobs) and starts
//! the shared job worker immediately (run-9 ruling for explicit
//! requests); everything the engine does still flows through the queue.

use std::sync::atomic::Ordering;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};
use crate::dbops::JobsWorker;

/// Ranked opening-triage report for `player` (identity-resolved: lexical
/// name variants + declared aliases). Pure database walk, engine off.
#[tauri::command]
pub async fn triage_report(
    state: State<'_, DbState>,
    player: String,
    max_games: Option<u32>,
) -> Result<kibitz_db::triage::TriageReport, String> {
    with_conn(&state, |conn| {
        let mut opts = kibitz_db::triage::TriageOptions::default();
        if let Some(n) = max_games {
            opts.max_games = n.max(1);
        }
        kibitz_db::triage::triage_report(conn, &player, &opts).map_err(|e| e.to_string())
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendStarted {
    pub job_id: i64,
    /// False when an existing pending/running/done job for the same FEN
    /// was reused (idempotent enqueue).
    pub created: bool,
    /// True: a worker is draining the queue (spawned by this call or
    /// already running and about to pick the job up).
    pub worker_active: bool,
}

/// Validate the FEN and enqueue the extension job idempotently.
pub(crate) fn enqueue_extend_impl(
    conn: &Connection,
    fen: &str,
    multipv: u32,
    depth: u32,
) -> Result<(i64, bool), String> {
    let _board: cozy_chess::Board = fen.parse().map_err(|e| format!("bad FEN: {e:?}"))?;
    kibitz_db::jobs::enqueue_book_extension(conn, fen, multipv, depth).map_err(|e| e.to_string())
}

/// Request a book extension for a GAP/FRONTIER position: enqueue the
/// MultiPV job AND start the worker (the click IS the explicit engine
/// request). Defaults: 4 lines at depth 30, both configurable.
#[tauri::command]
pub async fn triage_extend(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
    fen: String,
    multipv: Option<u32>,
    depth: Option<u32>,
) -> Result<ExtendStarted, String> {
    // Resolve the engine FIRST: a missing binary must be an honest error
    // before anything lands in the queue.
    let engine_path = kibitz_db::engine::resolve_engine_path().ok_or_else(|| {
        "no engine found (set KIBITZ_STOCKFISH, add tools/stockfish, or put stockfish on PATH)"
            .to_string()
    })?;
    let multipv = multipv
        .unwrap_or(kibitz_db::triage::EXTENSION_MULTIPV)
        .clamp(1, 8);
    let depth = depth
        .unwrap_or(kibitz_db::triage::EXTENSION_DEPTH)
        .clamp(8, 60);
    let (job_id, created) = with_conn(&state, |conn| {
        enqueue_extend_impl(conn, &fen, multipv, depth)
    })?;
    let db_path = crate::dbops::current_db_path(&state)?;
    crate::dbops::spawn_worker_if_idle(db_path, engine_path, &worker);
    Ok(ExtendStarted {
        job_id,
        created,
        worker_active: true,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatus {
    /// The stored result once one exists (survives restarts).
    pub extension: Option<kibitz_db::triage::BookExtension>,
    /// Status of the most recent book-extension job for this FEN:
    /// "pending" | "running" | "done" | "failed"; null when none exists.
    pub job_status: Option<String>,
    /// Queue rows (pending/running) ahead of a pending job — the honest
    /// "N jobs before yours" figure. 0 when not pending.
    pub jobs_ahead: i64,
    pub worker_active: bool,
}

pub(crate) fn extension_status_impl(
    conn: &Connection,
    fen: &str,
    worker_active: bool,
) -> Result<ExtensionStatus, String> {
    let extension =
        kibitz_db::triage::latest_book_extension(conn, fen).map_err(|e| e.to_string())?;
    let job: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, status FROM jobs
             WHERE purpose = 'book-extension'
               AND json_extract(payload, '$.fen') = ?1
             ORDER BY id DESC LIMIT 1",
            [fen],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (job_status, jobs_ahead) = match job {
        Some((id, status)) => {
            let ahead = if status == "pending" {
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs
                     WHERE status IN ('pending', 'running') AND id < ?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?
            } else {
                0
            };
            (Some(status), ahead)
        }
        None => (None, 0),
    };
    Ok(ExtensionStatus {
        extension,
        job_status,
        jobs_ahead,
        worker_active,
    })
}

/// Poll the state of a position's extension: stored result, job status,
/// and how many queue rows are ahead of it.
#[tauri::command]
pub async fn triage_extension_status(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
    fen: String,
) -> Result<ExtensionStatus, String> {
    let active = worker.active.load(Ordering::SeqCst);
    with_conn(&state, |conn| extension_status_impl(conn, &fen, active))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    const SICILIAN: &str = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";

    #[test]
    fn extend_validates_fen_and_enqueues_idempotently() {
        let (_dir, conn) = open_db();
        assert!(enqueue_extend_impl(&conn, "not a fen", 4, 30).is_err());

        let (id, created) = enqueue_extend_impl(&conn, SICILIAN, 4, 30).unwrap();
        assert!(created);
        let (id2, created2) = enqueue_extend_impl(&conn, SICILIAN, 4, 30).unwrap();
        assert_eq!((id2, created2), (id, false), "json_extract dedup");
        let (pending, ..) = kibitz_db::jobs::counts(&conn).unwrap();
        assert_eq!(pending, 1);
        assert_eq!(kibitz_db::engine::spawn_count(), 0, "enqueue only");
    }

    #[test]
    fn extension_status_reports_queue_position_then_result() {
        let (_dir, conn) = open_db();

        // Nothing yet: all-null honesty.
        let s = extension_status_impl(&conn, SICILIAN, false).unwrap();
        assert!(s.extension.is_none() && s.job_status.is_none());
        assert_eq!(s.jobs_ahead, 0);

        // An unrelated job sits ahead in the FIFO queue.
        conn.execute(
            "INSERT INTO jobs (purpose, payload) VALUES ('reanalyze', '{}')",
            [],
        )
        .unwrap();
        let (id, _) = enqueue_extend_impl(&conn, SICILIAN, 4, 30).unwrap();
        let s = extension_status_impl(&conn, SICILIAN, true).unwrap();
        assert_eq!(s.job_status.as_deref(), Some("pending"));
        assert_eq!(s.jobs_ahead, 1, "the reanalyze job runs first");
        assert!(s.worker_active);

        // Job completes and the result is stored durably.
        conn.execute("UPDATE jobs SET status = 'done' WHERE id = ?1", [id])
            .unwrap();
        let lines = vec![kibitz_db::triage::CandidateLine {
            sans: vec!["Nf3".into(), "d6".into(), "d4".into()],
            score_cp: 35,
            mate: None,
        }];
        kibitz_db::triage::store_book_extension(&conn, SICILIAN, "Stockfish 17", 30, 4, &lines)
            .unwrap();
        let s = extension_status_impl(&conn, SICILIAN, false).unwrap();
        assert_eq!(s.job_status.as_deref(), Some("done"));
        assert_eq!(s.jobs_ahead, 0);

        // Wire shape: camelCase.
        let json = serde_json::to_string(&s).unwrap();
        for needle in ["\"jobStatus\":", "\"jobsAhead\":", "\"workerActive\":"] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }

        let ext = s.extension.expect("stored result visible");
        assert_eq!(ext.lines, lines);
        assert_eq!((ext.depth, ext.multipv), (30, 4));
        assert_eq!(kibitz_db::engine::spawn_count(), 0);
    }
}
