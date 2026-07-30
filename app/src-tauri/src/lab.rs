//! Opening Lab IPC (run 11): cohort listing, the diagnosis report,
//! candidate structure-fit, and the cohort-scoped re-analysis batch.
//!
//! Product principle (CLAUDE.md #6): the report, the cohort listing and
//! the fit join are static database walks — no engine. The ONE engine
//! entry point here is `lab_reanalyze_start`, an explicit user click that
//! enqueues bounded re-analysis jobs for the cohort's unanalyzed games
//! through the existing queue AND starts the shared worker (the run-9
//! ruling for explicit requests). Branch extensions reuse the existing
//! `triage_extend` / `triage_extension_status` commands unchanged.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};
use crate::dbops::JobsWorker;

/// The user's opening families with game counts (identity-resolved) —
/// the cohort picker's rows.
#[tauri::command]
pub async fn lab_cohorts(
    state: State<'_, DbState>,
    player: String,
) -> Result<Vec<kibitz_db::opening_lab::CohortRow>, String> {
    with_conn(&state, |conn| {
        kibitz_db::opening_lab::cohorts(conn, &player).map_err(|e| e.to_string())
    })
}

/// The full Lab report for one cohort: verdict numbers, damage-ranked
/// branch table, homework list. Static walk, engine off.
#[tauri::command]
pub async fn lab_report(
    state: State<'_, DbState>,
    player: String,
    color: String,
    ecos: Vec<String>,
) -> Result<kibitz_db::opening_lab::LabReport, String> {
    with_conn(&state, |conn| {
        kibitz_db::opening_lab::lab_report(conn, &player, &color, &ecos)
            .map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// candidate fit: candidate line → structures → cached profile scores
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitFlag {
    pub flag: String,
    /// The user's score in this structure per the CACHED profile; null
    /// when the profile has no games in it.
    pub score_pct: Option<f64>,
    pub games: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineFit {
    /// Structures the line leads toward (empty = no distinctive
    /// structure at the line's end).
    pub flags: Vec<FitFlag>,
    /// False when no cached profile exists — the UI says "build a
    /// profile to see fit" instead of inventing a score.
    pub fit_available: bool,
    pub profile_player: Option<String>,
    pub profile_built_at: Option<String>,
}

pub(crate) fn line_fit_impl(conn: &Connection, fen: &str, sans: &[String]) -> Result<LineFit, String> {
    let structures =
        kibitz_db::opening_lab::candidate_structures(fen, sans).map_err(|e| e.to_string())?;

    // The cached self profile (meta profile_cache_self, home.rs contract).
    let cache: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'profile_cache_self'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let envelope: Option<serde_json::Value> =
        cache.and_then(|json| serde_json::from_str(&json).ok());

    let (fit_available, profile_player, profile_built_at) = match &envelope {
        Some(v) => (
            true,
            v["player"].as_str().map(str::to_string),
            v["builtAt"].as_str().map(str::to_string),
        ),
        None => (false, None, None),
    };

    let flags = structures
        .into_iter()
        .map(|flag| {
            let row = envelope
                .as_ref()
                .and_then(|v| v["profile"]["structures"].as_array())
                .and_then(|rows| rows.iter().find(|s| s["flag"].as_str() == Some(&flag)));
            FitFlag {
                score_pct: row.and_then(|s| s["score_pct"].as_f64()),
                games: row.and_then(|s| s["games"].as_u64()).unwrap_or(0) as u32,
                flag,
            }
        })
        .collect();

    Ok(LineFit {
        flags,
        fit_available,
        profile_player,
        profile_built_at,
    })
}

/// Where does a candidate line lead, structurally, and how does the user
/// score there? Plays `sans` from `fen` (the branch node, user to move)
/// and joins the resulting structure flags against the cached profile.
/// No profile cache → `fitAvailable: false`, honestly.
#[tauri::command]
pub async fn lab_line_fit(
    state: State<'_, DbState>,
    fen: String,
    sans: Vec<String>,
) -> Result<LineFit, String> {
    with_conn(&state, |conn| line_fit_impl(conn, &fen, &sans))
}

// ---------------------------------------------------------------------------
// cohort re-analysis: estimate → confirm → enqueue + run
// ---------------------------------------------------------------------------

/// Cohort games that are unanalyzed AND not already covered by a
/// reanalyze job (pending/running/done — failed jobs retry), with their
/// ply counts for the estimate.
fn reanalyze_targets(
    conn: &Connection,
    player: &str,
    color: &str,
    ecos: &[String],
) -> Result<Vec<(i64, i64)>, String> {
    let is_white = match color {
        "white" => true,
        "black" => false,
        other => return Err(format!("color must be \"white\" or \"black\", got {other:?}")),
    };
    let unanalyzed = kibitz_db::opening_lab::cohort_unanalyzed(conn, player, is_white, ecos)
        .map_err(|e| e.to_string())?;
    let uncovered: HashSet<i64> =
        kibitz_db::jobs::games_without_job(conn, kibitz_db::jobs::Purpose::Reanalyze)
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
    let mut out = Vec::new();
    for game_id in unanalyzed {
        if !uncovered.contains(&game_id) {
            continue;
        }
        let plies: i64 = conn
            .query_row(
                "SELECT ply_count FROM games WHERE id = ?1",
                [game_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        out.push((game_id, plies));
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabReanalyzeEstimate {
    /// Unanalyzed cohort games the batch would cover (already-queued/done
    /// games are excluded — starting is idempotent).
    pub games: i64,
    /// Queue rows the start would add (one bounded eval per position).
    pub jobs: i64,
    pub total_estimate_ms: f64,
    /// How the estimate was obtained — shown verbatim (honesty string,
    /// same contract as the database-wide batches).
    pub estimate_basis: String,
}

pub(crate) fn reanalyze_estimate_impl(
    conn: &Connection,
    player: &str,
    color: &str,
    ecos: &[String],
) -> Result<LabReanalyzeEstimate, String> {
    let targets = reanalyze_targets(conn, player, color, ecos)?;
    let jobs: i64 = targets.iter().map(|(_, p)| p).sum();
    let total_estimate_ms =
        jobs as f64 * (crate::dbops::UI_NODES as f64) / crate::dbops::ASSUMED_NODES_PER_SEC
            * 1000.0;
    Ok(LabReanalyzeEstimate {
        games: targets.len() as i64,
        jobs,
        total_estimate_ms,
        estimate_basis: format!(
            "assumed: {} nodes/position at {} nodes/s (engine off; \
             measuring the real speed would spawn it)",
            crate::dbops::UI_NODES,
            crate::dbops::ASSUMED_NODES_PER_SEC as u64
        ),
    })
}

/// Estimate the cohort re-analysis batch. No engine, no writes.
#[tauri::command]
pub async fn lab_reanalyze_estimate(
    state: State<'_, DbState>,
    player: String,
    color: String,
    ecos: Vec<String>,
) -> Result<LabReanalyzeEstimate, String> {
    with_conn(&state, |conn| {
        reanalyze_estimate_impl(conn, &player, &color, &ecos)
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabReanalyzeStarted {
    /// Games newly enqueued by this start (0 on a redundant re-start).
    pub games_enqueued: u32,
    /// Queue rows added (one per mainline position).
    pub jobs_enqueued: u32,
    pub pending: i64,
    /// True: a worker is draining the queue (spawned by this call or
    /// already running and about to pick the jobs up).
    pub worker_active: bool,
}

pub(crate) fn reanalyze_enqueue_impl(
    conn: &Connection,
    player: &str,
    color: &str,
    ecos: &[String],
) -> Result<(u32, u32, i64), String> {
    let targets = reanalyze_targets(conn, player, color, ecos)?;
    let mut games = 0u32;
    let mut jobs = 0u32;
    for (game_id, _) in targets {
        let n = kibitz_db::jobs::enqueue_reanalyze(conn, game_id, crate::dbops::UI_NODES)
            .map_err(|e| e.to_string())?;
        if n > 0 {
            games += 1;
            jobs += n;
        }
    }
    let (pending, ..) = kibitz_db::jobs::counts(conn).map_err(|e| e.to_string())?;
    Ok((games, jobs, pending))
}

/// Start the cohort re-analysis: enqueue one bounded eval per mainline
/// position of every uncovered unanalyzed cohort game AND start the job
/// worker — the click IS the explicit engine request (CLAUDE.md #6/#8).
#[tauri::command]
pub async fn lab_reanalyze_start(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
    player: String,
    color: String,
    ecos: Vec<String>,
) -> Result<LabReanalyzeStarted, String> {
    // Resolve the engine FIRST: a missing binary must be an honest error
    // before anything lands in the queue.
    let engine_path = kibitz_db::engine::resolve_engine_path().ok_or_else(|| {
        "no engine found (set KIBITZ_STOCKFISH, add tools/stockfish, or put stockfish on PATH)"
            .to_string()
    })?;
    let (games_enqueued, jobs_enqueued, pending) = with_conn(&state, |conn| {
        reanalyze_enqueue_impl(conn, &player, &color, &ecos)
    })?;
    let db_path = crate::dbops::current_db_path(&state)?;
    crate::dbops::spawn_worker_if_idle(db_path, engine_path, &worker);
    Ok(LabReanalyzeStarted {
        games_enqueued,
        jobs_enqueued,
        pending,
        worker_active: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
    use rusqlite::params;
    use std::io::Cursor;

    /// Two cohort games as White (Ruy): G1 gets evals, G2 stays
    /// unanalyzed — the re-analysis target.
    const FIXTURE: &str = r#"[Event "Club"]
[White "Lab, Tester"]
[Black "Erste, Anna"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Rg8 6. d4 h6 1-0

[Event "Club"]
[White "Lab, Tester"]
[Black "Zweite, Bea"]
[Result "0-1"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Rg8 6. d4 exd4 0-1
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 2, "failures: {:?}", st.failures);
        // G1: one legacy eval pair around White's ply 11 → analyzed.
        for (ply, cp) in [(10i64, 20i64), (11, 15)] {
            conn.execute(
                "INSERT INTO analyses (game_id, ply, kind, engine, eval_cp)
                 VALUES (1, ?1, 'legacy-import', 'Old Engine', ?2)",
                params![ply, cp],
            )
            .unwrap();
        }
        let mut stmt = conn
            .prepare("SELECT DISTINCT substr(eco,1,3) FROM games WHERE eco IS NOT NULL")
            .unwrap();
        let ecos: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        drop(stmt);
        (dir, conn, ecos)
    }

    #[test]
    fn estimate_counts_only_uncovered_unanalyzed_cohort_games() {
        let (_dir, conn, ecos) = fixture_db();

        let est = reanalyze_estimate_impl(&conn, "Lab, Tester", "white", &ecos).unwrap();
        assert_eq!(est.games, 1, "only the eval-less G2");
        assert_eq!(est.jobs, 12, "one job per mainline position of G2");
        // 12 positions × 200k nodes at the assumed speed = 1.6 s.
        assert!((est.total_estimate_ms - 1600.0).abs() < 1.0, "{}", est.total_estimate_ms);
        assert!(est.estimate_basis.starts_with("assumed"), "{}", est.estimate_basis);

        // Bad inputs fail cleanly.
        assert!(reanalyze_estimate_impl(&conn, "Lab, Tester", "purple", &ecos).is_err());
        assert!(reanalyze_estimate_impl(&conn, "Nobody, At All", "white", &ecos).is_err());
        assert_eq!(kibitz_db::engine::spawn_count(), 0, "estimates never spawn");
    }

    #[test]
    fn enqueue_targets_the_unanalyzed_game_idempotently() {
        let (_dir, conn, ecos) = fixture_db();

        let (games, jobs, pending) =
            reanalyze_enqueue_impl(&conn, "Lab, Tester", "white", &ecos).unwrap();
        assert_eq!((games, jobs), (1, 12));
        assert_eq!(pending, 12);
        // The jobs target G2 only.
        let covered: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT json_extract(payload, '$.game_id'))
                 FROM jobs WHERE purpose = 'reanalyze'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(covered, 1);
        let target: i64 = conn
            .query_row(
                "SELECT DISTINCT json_extract(payload, '$.game_id')
                 FROM jobs WHERE purpose = 'reanalyze'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(target, 2, "G2 is the unanalyzed game");

        // Re-start: G2 is now covered by pending jobs — nothing added.
        let (games, jobs, pending) =
            reanalyze_enqueue_impl(&conn, "Lab, Tester", "white", &ecos).unwrap();
        assert_eq!((games, jobs, pending), (0, 0, 12));
        let est = reanalyze_estimate_impl(&conn, "Lab, Tester", "white", &ecos).unwrap();
        assert_eq!((est.games, est.jobs), (0, 0), "estimate reflects coverage");

        // Enqueue-only: nothing ran, no engine spawned (CLAUDE.md #6).
        assert_eq!(kibitz_db::engine::spawn_count(), 0);

        // Wire shape: camelCase.
        let started = LabReanalyzeStarted {
            games_enqueued: 1,
            jobs_enqueued: 12,
            pending: 12,
            worker_active: true,
        };
        let json = serde_json::to_string(&started).unwrap();
        for needle in ["\"gamesEnqueued\":", "\"jobsEnqueued\":", "\"workerActive\":"] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }
    }

    #[test]
    fn line_fit_joins_the_cached_profile_or_degrades_honestly() {
        let (_dir, conn, _ecos) = fixture_db();
        // The 2...Nc6 node; the line exchanges into doubled white b-pawns.
        let fen = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 4 3";
        let sans: Vec<String> = ["Bc4", "Na5", "Bd5", "c6", "Bb3", "Nxb3", "axb3"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // No cached profile: flags computed, fit honestly unavailable.
        let fit = line_fit_impl(&conn, fen, &sans).unwrap();
        assert!(!fit.fit_available);
        assert_eq!(fit.profile_player, None);
        assert!(fit
            .flags
            .iter()
            .any(|f| f.flag == "own-doubled-pawns" && f.score_pct.is_none()));

        // Plant a profile cache (the home.rs envelope shape): the flag
        // now joins to the stored score; unknown flags stay null.
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('profile_cache_self', ?1)",
            [serde_json::json!({
                "player": "Lab, Tester",
                "builtAt": "2026-07-30 12:00:00",
                "profile": { "structures": [
                    { "flag": "own-doubled-pawns", "games": 7, "score_pct": 28.6, "examples": [] }
                ]}
            })
            .to_string()],
        )
        .unwrap();
        let fit = line_fit_impl(&conn, fen, &sans).unwrap();
        assert!(fit.fit_available);
        assert_eq!(fit.profile_player.as_deref(), Some("Lab, Tester"));
        assert_eq!(fit.profile_built_at.as_deref(), Some("2026-07-30 12:00:00"));
        let doubled = fit
            .flags
            .iter()
            .find(|f| f.flag == "own-doubled-pawns")
            .expect("flag present");
        assert_eq!((doubled.score_pct, doubled.games), (Some(28.6), 7));

        // Wire shape + honest failure on a bad FEN; engine never spawned.
        let json = serde_json::to_string(&fit).unwrap();
        for needle in ["\"fitAvailable\":", "\"scorePct\":", "\"profilePlayer\":"] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }
        assert!(line_fit_impl(&conn, "not a fen", &sans).is_err());
        assert_eq!(kibitz_db::engine::spawn_count(), 0);
    }
}
