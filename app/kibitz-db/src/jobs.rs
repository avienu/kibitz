//! Persistent, resumable analysis job queue (SQLite-backed).
//!
//! Product principle (CLAUDE.md #6): the engine runs ONLY when a job is
//! executed, and jobs exist only because a WSUI screen fired, a user asked
//! for analysis, or a user started a batch. Nothing here spawns an engine
//! on its own initiative.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    WsuiConfirm,
    UserAnalysis,
    BatchAnnotate,
    BatchProfile,
    Reanalyze,
    /// Deep MultiPV analysis of a triage GAP/FRONTIER position, producing
    /// candidate lines to adopt into the repertoire (run 10).
    BookExtension,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::WsuiConfirm => "wsui-confirm",
            Purpose::UserAnalysis => "user-analysis",
            Purpose::BatchAnnotate => "batch-annotate",
            Purpose::BatchProfile => "batch-profile",
            Purpose::Reanalyze => "reanalyze",
            Purpose::BookExtension => "book-extension",
        }
    }
}

/// Payload of an engine job: verify/refute a fired alert, or evaluate a
/// position for the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnginePayload {
    pub fen: String,
    pub nodes: u64,
    /// For wsui-confirm: which side the fired alert claims is winning
    /// material ("white"/"black"), and the game/ply it belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ply: Option<u32>,
}

/// Payload of a `batch-annotate` job: statically annotate one game inside
/// the worker. The annotate pass itself spawns NO engine — it only
/// enqueues bounded wsui-confirm jobs, which the same worker run then
/// drains (lazily spawning the engine only if any exist).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatePayload {
    pub game_id: i64,
    /// Node budget for the confirm jobs the annotate pass enqueues.
    pub nodes: u64,
    /// Inline-comment cap for the game's narration pass.
    pub max_comments: u32,
}

/// Payload of a `book-extension` job: one deep MultiPV search of a
/// position where the user's book ends. Defaults come from
/// `triage::EXTENSION_MULTIPV` / `EXTENSION_DEPTH` (4 lines, depth 30);
/// both are caller-configurable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookExtensionPayload {
    pub fen: String,
    pub multipv: u32,
    pub depth: u32,
}

/// Enqueue a book-extension job for `fen`, idempotently: an existing
/// pending/running/done job for the same FEN is reused (the json_extract
/// dedup pattern); failed jobs do NOT count, so a retry re-enqueues.
/// Returns `(job_id, created)`. Enqueue-only — nothing runs until a
/// worker is started.
pub fn enqueue_book_extension(
    conn: &Connection,
    fen: &str,
    multipv: u32,
    depth: u32,
) -> anyhow::Result<(i64, bool)> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM jobs
             WHERE purpose = 'book-extension'
               AND status IN ('pending', 'running', 'done')
               AND json_extract(payload, '$.fen') = ?1
             ORDER BY id DESC LIMIT 1",
            [fen],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok((id, false));
    }
    conn.execute(
        "INSERT INTO jobs (purpose, payload) VALUES (?1, ?2)",
        params![
            Purpose::BookExtension.as_str(),
            serde_json::to_string(&BookExtensionPayload {
                fen: fen.to_string(),
                multipv,
                depth,
            })?
        ],
    )?;
    Ok((conn.last_insert_rowid(), true))
}

pub fn enqueue(
    conn: &Connection,
    purpose: Purpose,
    payload: &EnginePayload,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO jobs (purpose, payload) VALUES (?1, ?2)",
        params![purpose.as_str(), serde_json::to_string(payload)?],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Startup resumability: anything left 'running' by a dead worker becomes
/// 'pending' again.
pub fn reset_running(conn: &Connection) -> anyhow::Result<u64> {
    Ok(conn.execute(
        "UPDATE jobs SET status = 'pending', updated_at = datetime('now')
         WHERE status = 'running'",
        [],
    )? as u64)
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub done: u32,
    pub failed: u32,
}

/// Execute pending jobs serially with one engine process (spawned lazily —
/// zero pending jobs means zero spawns). Returns after `max_jobs`.
pub fn run_pending(
    conn: &Connection,
    engine_path: &Path,
    max_jobs: u32,
) -> anyhow::Result<RunReport> {
    run_pending_until(conn, engine_path, max_jobs, None)
}

/// [`run_pending`] with a cooperative stop flag, checked between jobs: when
/// it flips true the worker returns promptly and every unstarted job stays
/// 'pending' — pausing a batch is stopping the worker; restarting resumes
/// exactly where it left off (the queue is the state).
pub fn run_pending_until(
    conn: &Connection,
    engine_path: &Path,
    max_jobs: u32,
    stop: Option<&AtomicBool>,
) -> anyhow::Result<RunReport> {
    let mut report = RunReport::default();
    let mut engine: Option<Engine> = None;

    for _ in 0..max_jobs {
        if stop.is_some_and(|s| s.load(Ordering::SeqCst)) {
            break;
        }
        let next: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT id, purpose, payload FROM jobs WHERE status = 'pending'
                 ORDER BY id LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let Some((id, purpose, payload)) = next else {
            break;
        };
        conn.execute(
            "UPDATE jobs SET status='running', updated_at=datetime('now') WHERE id=?1",
            [id],
        )?;
        let outcome = (|| -> anyhow::Result<serde_json::Value> {
            // Batch-annotate jobs are static: no engine, no EnginePayload.
            if purpose == "batch-annotate" {
                let p: AnnotatePayload = serde_json::from_str(&payload)?;
                let r = crate::annotate::annotate_game(conn, p.game_id, p.nodes, p.max_comments)?;
                return Ok(serde_json::json!({
                    "game_id": p.game_id,
                    "positions_analyzed": r.positions_analyzed,
                    "screens_fired": r.screens_fired,
                    "jobs_enqueued": r.jobs_enqueued,
                    "comments_added": r.comments_added,
                }));
            }
            // Book-extension jobs run a deep MultiPV search and persist
            // the candidate lines durably (book_extensions, run 10).
            if purpose == "book-extension" {
                let p: BookExtensionPayload = serde_json::from_str(&payload)?;
                if engine.is_none() {
                    engine = Some(Engine::spawn(engine_path)?);
                }
                let e = engine.as_mut().expect("just spawned");
                let identity = e.identity.clone();
                let raw = e.eval_depth_multipv(&p.fen, p.multipv, p.depth)?;
                let lines = crate::triage::candidate_lines(&p.fen, &raw)?;
                let extension_id = crate::triage::store_book_extension(
                    conn, &p.fen, &identity, p.depth, p.multipv, &lines,
                )?;
                return Ok(serde_json::json!({
                    "extension_id": extension_id,
                    "fen": p.fen,
                    "lines": lines.len(),
                    "depth": p.depth,
                    "multipv": p.multipv,
                    "engine": identity,
                }));
            }
            let p: EnginePayload = serde_json::from_str(&payload)?;
            if engine.is_none() {
                engine = Some(Engine::spawn(engine_path)?);
            }
            let e = engine.as_mut().expect("just spawned");
            let identity = e.identity.clone();
            let line = e.eval_nodes(&p.fen, p.nodes)?;
            let mut result = serde_json::json!({
                "score_cp": line.score_cp,
                "mate": line.mate,
                "pv": line.pv,
                "nodes": p.nodes,
                "engine": identity,
            });
            // Any evaluation tied to a stored game position becomes a
            // first-class 'fresh' analysis row, engine identity stamped
            // (verdict 3a). Legacy rows are never touched.
            if let (Some(game_id), Some(ply)) = (p.game_id, p.ply) {
                conn.execute(
                    "INSERT INTO analyses
                       (game_id, ply, kind, engine, nodes, eval_cp, pv)
                     VALUES (?1, ?2, 'fresh', ?3, ?4, ?5, ?6)",
                    params![
                        game_id,
                        ply as i64,
                        identity,
                        p.nodes as i64,
                        line.score_cp,
                        serde_json::to_string(&line.pv)?
                    ],
                )?;
            }
            if purpose == "wsui-confirm" {
                // The eval is from the side to move's POV; translate to the
                // claimed beneficiary and grade the alert. Mate distances
                // convert with the same sign flips and never masquerade as
                // centipawns downstream.
                let stm_is_white = p.fen.split_whitespace().nth(1) == Some("w");
                let flip = |v: i32| -> i32 {
                    let white = if stm_is_white { v } else { -v };
                    match p.beneficiary.as_deref() {
                        Some("black") => -white,
                        _ => white,
                    }
                };
                let benef_cp = flip(line.score_cp);
                let benef_mate = line.mate.map(flip);
                let status = if benef_mate.is_some_and(|m| m >= 0) || benef_cp >= 150 {
                    "confirmed"
                } else if benef_mate.is_some_and(|m| m < 0) || benef_cp <= 50 {
                    "refuted"
                } else {
                    "unclear-at-budget"
                };
                result["status"] = serde_json::json!(status);
                result["score_delta_cp"] = serde_json::json!(benef_cp);
                result["mate_for_beneficiary"] = serde_json::json!(benef_mate);

                // Suggestion verification (run 11, maintainer ruling:
                // "at least a cursory engine review, at least if tactics
                // screen is present"). The fired screen already
                // sanctioned this engine run; the confirm search above
                // doubles as the baseline, and each static candidate
                // gets one cursory bounded search of the position after
                // it (≤3 candidates + shared baseline ≤ 4 searches).
                // Narration then renders only cleared moves at this ply.
                if let Ok(board) = p.fen.parse::<cozy_chess::Board>() {
                    let record = kibitz_core::analyze(&board);
                    let suggestions = kibitz_core::suggest::suggest(&record, &board);
                    if record.wsui.screen_fired && !suggestions.is_empty() {
                        let baseline =
                            crate::verify::fold_score(Some(line.score_cp), line.mate).unwrap_or(0);
                        let mut cands = Vec::with_capacity(suggestions.len());
                        for s in &suggestions {
                            let score = s
                                .mv
                                .parse::<cozy_chess::Move>()
                                .ok()
                                .filter(|mv| board.is_legal(*mv))
                                .and_then(|mv| {
                                    let mut b2 = board.clone();
                                    b2.play(mv);
                                    e.eval_nodes(&b2.to_string(), crate::verify::VERIFY_NODES)
                                        .ok()
                                })
                                // The child search is the OPPONENT's POV.
                                .and_then(|l| crate::verify::fold_score(Some(l.score_cp), l.mate))
                                .map(|cp| -cp);
                            cands.push(crate::verify::CandidateEval {
                                uci: s.mv.clone(),
                                static_risk: s.static_risk,
                                score,
                            });
                        }
                        result["cleared_suggestions"] =
                            serde_json::json!(crate::verify::cleared_moves(baseline, &cands));
                        result["verify_nodes"] = serde_json::json!(crate::verify::VERIFY_NODES);
                    }
                }
            }
            Ok(result)
        })();
        match outcome {
            Ok(result) => {
                conn.execute(
                    "UPDATE jobs SET status='done', result=?1, updated_at=datetime('now')
                     WHERE id=?2",
                    params![result.to_string(), id],
                )?;
                report.done += 1;
            }
            Err(e) => {
                conn.execute(
                    "UPDATE jobs SET status='failed', result=?1, updated_at=datetime('now')
                     WHERE id=?2",
                    params![format!("{e}"), id],
                )?;
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

/// Enqueue a full-game re-analysis: one bounded eval per mainline
/// position (verdict 3d). Fresh results are preferred for display; legacy
/// imported analyses are retained untouched.
pub fn enqueue_reanalyze(conn: &Connection, game_id: i64, nodes: u64) -> anyhow::Result<u32> {
    let (start, tokens) = crate::edit::game_tokens(conn, game_id)?;
    let mut board = start.clone();
    let mut count = 0u32;
    let mut ply = 0u32;
    for p in crate::movebin::mainline_of(&tokens) {
        match p {
            crate::movebin::Ply::Move(m) => {
                board.play(m);
                ply += 1;
            }
            crate::movebin::Ply::Null => break,
        }
        enqueue(
            conn,
            Purpose::Reanalyze,
            &EnginePayload {
                fen: board.to_string(),
                nodes,
                beneficiary: None,
                game_id: Some(game_id),
                ply: Some(ply),
            },
        )?;
        count += 1;
    }
    Ok(count)
}

/// Games not yet covered by a queued/running/completed job of `purpose`
/// (failed jobs do NOT count as covered — a re-start retries them).
/// Coverage is by the payload's `game_id`.
pub fn games_without_job(conn: &Connection, purpose: Purpose) -> anyhow::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT g.id FROM games g
         WHERE NOT EXISTS (
             SELECT 1 FROM jobs j
             WHERE j.purpose = ?1
               AND j.status IN ('pending', 'running', 'done')
               AND json_extract(j.payload, '$.game_id') = g.id)
         ORDER BY g.id",
    )?;
    let rows = stmt.query_map([purpose.as_str()], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// "Annotate database": enqueue one static `batch-annotate` job per game
/// not already covered. Idempotent — re-starting skips games with a
/// queued, running or completed batch-annotate job. Returns the number of
/// jobs enqueued. Nothing runs (and no engine exists) until a worker is
/// started.
pub fn enqueue_batch_annotate(
    conn: &Connection,
    nodes: u64,
    max_comments: u32,
) -> anyhow::Result<u32> {
    let games = games_without_job(conn, Purpose::BatchAnnotate)?;
    let mut enqueued = 0u32;
    for game_id in games {
        conn.execute(
            "INSERT INTO jobs (purpose, payload) VALUES (?1, ?2)",
            params![
                Purpose::BatchAnnotate.as_str(),
                serde_json::to_string(&AnnotatePayload {
                    game_id,
                    nodes,
                    max_comments,
                })?
            ],
        )?;
        enqueued += 1;
    }
    Ok(enqueued)
}

/// "Fresh analysis pass": for every game not yet covered by a reanalyze
/// job, enqueue one bounded eval per mainline position (the existing
/// `reanalyze` purpose). Idempotent per game. Returns (games, jobs).
pub fn enqueue_batch_fresh(conn: &Connection, nodes: u64) -> anyhow::Result<(u32, u32)> {
    let games = games_without_job(conn, Purpose::Reanalyze)?;
    let mut jobs = 0u32;
    let mut covered = 0u32;
    for game_id in games {
        // A game with an empty mainline enqueues nothing (and is not
        // counted — it will be harmlessly re-visited, enqueueing nothing).
        let n = enqueue_reanalyze(conn, game_id, nodes)?;
        if n > 0 {
            jobs += n;
            covered += 1;
        }
    }
    Ok((covered, jobs))
}

pub fn counts(conn: &Connection) -> anyhow::Result<(i64, i64, i64, i64)> {
    let one = |st: &str| -> rusqlite::Result<i64> {
        conn.query_row("SELECT COUNT(*) FROM jobs WHERE status = ?1", [st], |r| {
            r.get(0)
        })
    };
    Ok((
        one("pending")?,
        one("running")?,
        one("done")?,
        one("failed")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    #[test]
    fn book_extension_payload_round_trips() {
        let p = BookExtensionPayload {
            fen: "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2".into(),
            multipv: 4,
            depth: 30,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: BookExtensionPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fen, p.fen);
        assert_eq!((back.multipv, back.depth), (4, 30));
        // The dedup key the enqueue query extracts must be present.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["fen"], p.fen.as_str());
    }

    #[test]
    fn enqueue_book_extension_is_idempotent_by_fen_and_retries_failures() {
        let (_dir, conn) = open_db();
        let fen = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";

        let (id, created) = enqueue_book_extension(&conn, fen, 4, 30).unwrap();
        assert!(created);
        // Same FEN again: reused, no duplicate row (json_extract dedup).
        let (id2, created2) = enqueue_book_extension(&conn, fen, 4, 30).unwrap();
        assert_eq!((id2, created2), (id, false));
        let (pending, ..) = counts(&conn).unwrap();
        assert_eq!(pending, 1);

        // A DIFFERENT position enqueues normally.
        let other = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
        let (id3, created3) = enqueue_book_extension(&conn, other, 4, 30).unwrap();
        assert!(created3 && id3 != id);

        // done still counts as covered; failed does not (retry allowed).
        conn.execute("UPDATE jobs SET status = 'done' WHERE id = ?1", [id])
            .unwrap();
        let (id4, created4) = enqueue_book_extension(&conn, fen, 4, 30).unwrap();
        assert_eq!((id4, created4), (id, false));
        conn.execute("UPDATE jobs SET status = 'failed' WHERE id = ?1", [id])
            .unwrap();
        let (id5, created5) = enqueue_book_extension(&conn, fen, 4, 30).unwrap();
        assert!(created5 && id5 != id, "failed jobs are retried");

        // Enqueue-only: nothing ran, no engine was spawned (CLAUDE.md #6).
        assert_eq!(crate::engine::spawn_count(), 0);
    }
}
