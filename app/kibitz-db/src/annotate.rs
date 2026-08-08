//! Batch game annotation: run the static Kibitz analysis over a game's
//! mainline, enqueue bounded engine-confirmation jobs for fired screens,
//! and (re)generate the game's delta narrations.
//!
//! THE ENGINE IS NEVER SPAWNED HERE. Analysis is static; fired screens
//! only *enqueue* jobs (CLAUDE.md #6). Tests assert the spawn count.
//!
//! Prose generation itself lives in [`crate::narrate`], shared with
//! verdict fold-back so both paths tell one consistent, delta-driven
//! story (run-5 feedback item 2).

use rusqlite::Connection;

use crate::jobs::{enqueue, EnginePayload, Purpose};
use crate::movebin::Token;

#[derive(Debug, Default)]
pub struct AnnotateReport {
    pub positions_analyzed: u32,
    pub screens_fired: u32,
    /// Bounded wsui-confirm jobs enqueued for fired screens.
    pub jobs_enqueued: u32,
    /// Bounded suggest-verify jobs enqueued at quiet closing-eligible
    /// plies (2026-07-29 field report), so annotated games actually
    /// recommend moves. Annotate is an explicit user engine action
    /// (run-9 ruling); nothing runs until a worker starts.
    pub suggest_jobs_enqueued: u32,
    pub comments_added: u32,
}

/// Annotate one game. `confirm_nodes` is the bounded budget the enqueued
/// wsui-confirm jobs will use when (and only when) a worker runs.
pub fn annotate_game(
    conn: &Connection,
    game_id: i64,
    confirm_nodes: u64,
    max_comments: u32,
) -> anyhow::Result<AnnotateReport> {
    let (start, tokens) = crate::edit::game_tokens(conn, game_id)?;
    let mut report = AnnotateReport::default();

    // Walk the mainline enqueueing one bounded confirm per fired screen.
    let mut board = start.clone();
    let mut depth = 0u32;
    let mut ply_in_main = 0u32;
    // Don't re-enqueue a position that already has a completed verdict
    // (re-annotation after fold-back must not redo engine work).
    let existing = crate::narrate::load_verdicts(conn, game_id)?;

    for token in tokens.iter() {
        match token {
            Token::VarStart => depth += 1,
            Token::VarEnd => depth = depth.saturating_sub(1),
            Token::Null if depth == 0 => break,
            Token::Move(mv) if depth == 0 => {
                let board_before = board.clone();
                board.play(*mv);
                ply_in_main += 1;
                report.positions_analyzed += 1;
                let mut record = kibitz_core::analyze(&board);
                record
                    .wsui
                    .alerts
                    .retain(|a| a.severity >= kibitz_core::record::Severity::Medium);
                if !record.wsui.screen_fired {
                    // Quiet ply (2026-07-29 field report): where a
                    // narration closing would render — plans present,
                    // suggestions present, not mid-exchange — enqueue one
                    // bounded suggest-verify review so the closing has
                    // engine-cleared candidates to show. Fired plies get
                    // the same review via their wsui-confirm job instead.
                    // Enqueue-only; idempotent per (game, ply).
                    // A plan for the side TO MOVE, not merely a plan. We
                    // suggest moves for whoever is on move, so a ply whose
                    // only plans belong to the opponent buys a bounded
                    // engine job and nothing to spend it on. This used to
                    // read "any composite plan exists", which was
                    // indistinguishable while the sided-plan filter was
                    // dropping the opponent's plans for us. Correct either
                    // way, and it becomes load-bearing the moment that
                    // filter retires — engine work stays off by default
                    // (CLAUDE.md #6).
                    if record.composite_plans.iter().any(|c| {
                        let stm: kibitz_core::record::Favors = match board.side_to_move() {
                            cozy_chess::Color::White => kibitz_core::record::Favors::White,
                            cozy_chess::Color::Black => kibitz_core::record::Favors::Black,
                        };
                        c.favors == stm || c.favors == kibitz_core::record::Favors::Balanced
                    }) && !crate::narrate::is_capture_ply(&board_before, *mv)
                        && !kibitz_core::suggest::suggest(&record, &board).is_empty()
                    {
                        let (_, created) = crate::jobs::enqueue_suggest_verify(
                            conn,
                            game_id,
                            ply_in_main,
                            &record.fen,
                        )?;
                        if created {
                            report.suggest_jobs_enqueued += 1;
                        }
                    }
                    continue;
                }
                report.screens_fired += 1;
                // A suggest-verify-only verdict never covers a fired ply:
                // only a completed confirm verdict does.
                if existing
                    .get(&ply_in_main)
                    .is_some_and(|v| v.status.is_some())
                {
                    continue;
                }
                // One bounded confirm per fired position, attributed to
                // the most severe alert's owner — the OTHER side is the
                // beneficiary.
                if let Some(top) = record.wsui.alerts.first() {
                    let beneficiary = match top.side {
                        kibitz_core::record::SideColor::White => "black",
                        kibitz_core::record::SideColor::Black => "white",
                    };
                    enqueue(
                        conn,
                        Purpose::WsuiConfirm,
                        &EnginePayload {
                            fen: record.fen.clone(),
                            nodes: confirm_nodes,
                            beneficiary: Some(beneficiary.to_string()),
                            game_id: Some(game_id),
                            ply: Some(ply_in_main),
                        },
                    )?;
                    report.jobs_enqueued += 1;
                }
            }
            _ => {}
        }
    }

    let voice = crate::narrate::narration_voice(conn)?;
    report.comments_added =
        crate::narrate::narrate_game(conn, game_id, &existing, max_comments, voice)?;
    Ok(report)
}

/// Fold completed wsui-confirm verdicts back into the narrations
/// (run-4 goal 3): each touched game is re-narrated with its full verdict
/// set, so confirmed alerts lead with the engine's PV and refuted alerts
/// vanish. Jobs are marked with `folded_at` so folding is idempotent.
#[derive(Debug, Default)]
pub struct FoldReport {
    pub folded: u32,
    pub confirmed: u32,
    pub refuted: u32,
    pub unclear: u32,
}

pub fn fold_back(conn: &Connection) -> anyhow::Result<FoldReport> {
    let mut report = FoldReport::default();
    let jobs: Vec<(i64, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, purpose, payload, result FROM jobs
             WHERE purpose IN ('wsui-confirm', 'suggest-verify') AND status = 'done'
               AND folded_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<Result<_, _>>()?
    };

    let mut touched: Vec<i64> = Vec::new();
    for (job_id, purpose, payload, result) in jobs {
        // Suggestion reviews only re-narrate their game; the
        // confirmed/refuted alert grading below is wsui-confirm-only.
        let game_id = if purpose == "suggest-verify" {
            let p: crate::jobs::SuggestVerifyPayload = serde_json::from_str(&payload)?;
            Some(p.game_id)
        } else {
            let p: crate::jobs::EnginePayload = serde_json::from_str(&payload)?;
            let v: serde_json::Value = serde_json::from_str(&result)?;
            match v["status"].as_str() {
                Some("confirmed") => report.confirmed += 1,
                Some("refuted") => report.refuted += 1,
                _ => report.unclear += 1,
            }
            p.game_id
        };
        if let Some(game_id) = game_id {
            if !touched.contains(&game_id) {
                touched.push(game_id);
            }
        }
        conn.execute(
            "UPDATE jobs SET folded_at = datetime('now') WHERE id = ?1",
            [job_id],
        )?;
        report.folded += 1;
    }

    let voice = crate::narrate::narration_voice(conn)?;
    for game_id in touched {
        let verdicts = crate::narrate::load_verdicts(conn, game_id)?;
        crate::narrate::narrate_game(conn, game_id, &verdicts, u32::MAX, voice)?;
    }
    Ok(report)
}
