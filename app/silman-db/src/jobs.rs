//! Persistent, resumable analysis job queue (SQLite-backed).
//!
//! Product principle (CLAUDE.md #6): the engine runs ONLY when a job is
//! executed, and jobs exist only because a WSUI screen fired, a user asked
//! for analysis, or a user started a batch. Nothing here spawns an engine
//! on its own initiative.

use std::path::Path;

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
    let mut report = RunReport::default();
    let mut engine: Option<Engine> = None;

    for _ in 0..max_jobs {
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
