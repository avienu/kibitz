//! Persistent, resumable analysis job queue (SQLite-backed).
//!
//! Product principle (CLAUDE.md #6): the engine runs ONLY when a job is
//! executed, and jobs exist only because a WSUI screen fired, a user asked
//! for analysis, or a user started a batch. Nothing here spawns an engine
//! on its own initiative.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    WsuiConfirm,
    UserAnalysis,
    BatchAnnotate,
    BatchProfile,
    Reanalyze,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::WsuiConfirm => "wsui-confirm",
            Purpose::UserAnalysis => "user-analysis",
            Purpose::BatchAnnotate => "batch-annotate",
            Purpose::BatchProfile => "batch-profile",
            Purpose::Reanalyze => "reanalyze",
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
