//! Batch game annotation: run the static Silman analysis over a game's
//! mainline, insert template-mode comments inline (encoding v2), and
//! enqueue bounded engine-confirmation jobs for fired screens.
//!
//! THE ENGINE IS NEVER SPAWNED HERE. Analysis is static; fired screens
//! only *enqueue* jobs (CLAUDE.md #6). Tests assert the spawn count.

use rusqlite::Connection;
use silman_core::record::Magnitude;

use crate::jobs::{enqueue, EnginePayload, Purpose};
use crate::movebin::Token;

#[derive(Debug, Default)]
pub struct AnnotateReport {
    pub positions_analyzed: u32,
    pub screens_fired: u32,
    pub jobs_enqueued: u32,
    pub comments_added: u32,
}

/// Annotate one game in place. `confirm_nodes` is the bounded budget the
/// enqueued wsui-confirm jobs will use when (and only when) a worker runs.
pub fn annotate_game(
    conn: &Connection,
    game_id: i64,
    confirm_nodes: u64,
    max_comments: u32,
) -> anyhow::Result<AnnotateReport> {
    let (start, tokens) = crate::edit::game_tokens(conn, game_id)?;
    let mut report = AnnotateReport::default();

    // Walk the mainline, computing a record after every ply; collect the
    // comments to insert (position in the token stream -> text).
    let mut board = start.clone();
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut last_summary = String::new();
    let mut depth = 0u32;
    let mut ply_in_main = 0u32;

    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::VarStart => depth += 1,
            Token::VarEnd => depth = depth.saturating_sub(1),
            Token::Move(mv) if depth == 0 => {
                board.play(*mv);
                ply_in_main += 1;
                report.positions_analyzed += 1;
                let mut record = silman_core::analyze(&board);
                // Inline comments talk about what matters: drop low-severity
                // chatter before verbalizing (the full record remains
                // available to the UI via explain).
                record
                    .wsui
                    .alerts
                    .retain(|a| a.severity >= silman_core::record::Severity::Medium);
                let fired = record.wsui.screen_fired;
                if fired {
                    report.screens_fired += 1;
                    // One bounded confirm per fired position, attributed to
                    // the most severe alert's owner — the OTHER side is the
                    // beneficiary.
                    if let Some(top) = record.wsui.alerts.first() {
                        let beneficiary = match top.side {
                            silman_core::record::SideColor::White => "black",
                            silman_core::record::SideColor::Black => "white",
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
                // Comment when a screen fires, or when the positional
                // story (clear+ imbalances) changes.
                let mut summary: String = record
                    .imbalances
                    .iter()
                    .filter(|i| i.magnitude >= Magnitude::Clear)
                    .map(|i| format!("{:?}:{:?};", i.kind, i.favors))
                    .collect();
                for a in &record.wsui.alerts {
                    summary.push_str(&format!("{:?}@{:?};", a.kind, a.target));
                }
                // Comment only when the story CHANGES — a persisting alert
                // or imbalance is narrated once, not every ply.
                let notable = (fired || !summary.is_empty()) && summary != last_summary;
                if notable && report.comments_added < max_comments {
                    let prose = silman_verbalize::verbalize(&record);
                    // Compress paragraphs for an inline comment.
                    let text = prose.replace("\n\n", " ");
                    inserts.push((idx + 1, text));
                    report.comments_added += 1;
                }
                if notable {
                    last_summary = summary;
                }
            }
            Token::Null if depth == 0 => {
                // Static analysis across a null is meaningless; stop.
                break;
            }
            _ => {}
        }
    }

    // Apply insertions back-to-front so indices stay valid.
    let mut new_tokens = tokens;
    for (idx, text) in inserts.into_iter().rev() {
        new_tokens.insert(idx, Token::Comment(text));
    }
    crate::edit::update_game_tokens(conn, game_id, &new_tokens)?;
    Ok(report)
}

/// Fold completed wsui-confirm verdicts back into the stored annotations
/// (run-4 goal 3): a confirmed alert leads the re-rendered comment with
/// the engine's PV; a refuted alert is dropped from the prose entirely.
/// Jobs are marked with `folded_at` so folding is idempotent.
#[derive(Debug, Default)]
pub struct FoldReport {
    pub folded: u32,
    pub confirmed: u32,
    pub refuted: u32,
    pub unclear: u32,
}

pub fn fold_back(conn: &Connection) -> anyhow::Result<FoldReport> {
    use silman_core::record::{EngineCheck, EngineCheckStatus};

    let mut report = FoldReport::default();
    // Suppress narration of the SAME persisting weakness at consecutive
    // folded plies: a king attack confirmed eight moves running is one
    // story, not eight. Keyed per game by the leading alert's identity.
    let mut last_story: std::collections::HashMap<i64, String> = Default::default();
    let jobs: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, payload, result FROM jobs
             WHERE purpose = 'wsui-confirm' AND status = 'done'
               AND folded_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };

    for (job_id, payload, result) in jobs {
        let p: crate::jobs::EnginePayload = serde_json::from_str(&payload)?;
        let (Some(game_id), Some(ply)) = (p.game_id, p.ply) else {
            continue;
        };
        let v: serde_json::Value = serde_json::from_str(&result)?;
        let status = match v["status"].as_str() {
            Some("confirmed") => {
                report.confirmed += 1;
                EngineCheckStatus::Confirmed
            }
            Some("refuted") => {
                report.refuted += 1;
                EngineCheckStatus::Refuted
            }
            _ => {
                report.unclear += 1;
                EngineCheckStatus::UnclearAtBudget
            }
        };

        // Rebuild the record at that position and merge the verdict.
        let (start, mut tokens) = crate::edit::game_tokens(conn, game_id)?;
        let mut board = start.clone();
        let mut main_ply = 0u32;
        let mut move_idx: Option<usize> = None;
        let mut depth = 0u32;
        for (i, t) in tokens.iter().enumerate() {
            match t {
                Token::VarStart => depth += 1,
                Token::VarEnd => depth = depth.saturating_sub(1),
                Token::Move(m) if depth == 0 => {
                    board.play(*m);
                    main_ply += 1;
                    if main_ply == ply {
                        move_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(move_idx) = move_idx else { continue };

        let mut record = silman_core::analyze(&board);
        record
            .wsui
            .alerts
            .retain(|a| a.severity >= silman_core::record::Severity::Medium);
        // Convert the engine PV (UCI moves) to SAN for the record.
        let pv_san = {
            let mut b2 = board.clone();
            let mut sans = Vec::new();
            for uci in v["pv"].as_array().into_iter().flatten().take(3) {
                let Some(mv) = uci
                    .as_str()
                    .and_then(|u| u.parse::<cozy_chess::Move>().ok())
                else {
                    break;
                };
                if !b2.is_legal(mv) {
                    break;
                }
                sans.push(crate::san::format_san(&b2, mv));
                b2.play(mv);
            }
            sans
        };
        let check = EngineCheck {
            status,
            pv: pv_san,
            score_delta_cp: v["score_delta_cp"].as_i64().map(|x| x as i32),
            budget_nodes: v["nodes"].as_u64().unwrap_or(0),
        };
        match status {
            EngineCheckStatus::Refuted => {
                // The tactic does not work: drop the leading alert.
                if !record.wsui.alerts.is_empty() {
                    record.wsui.alerts.remove(0);
                }
                record.wsui.screen_fired = !record.wsui.alerts.is_empty();
            }
            _ => {
                if let Some(top) = record.wsui.alerts.first_mut() {
                    top.engine_check = Some(check);
                }
            }
        }

        // Replace (or insert) the comment right after the mainline move.
        let prose = silman_verbalize::verbalize(&record).replace("\n\n", " ");
        let has_content = record.wsui.screen_fired
            || record
                .imbalances
                .iter()
                .any(|i| i.magnitude >= Magnitude::Clear);
        let story = record
            .wsui
            .alerts
            .first()
            .map(|a| format!("{:?}@{:?}:{status:?}", a.kind, a.target))
            .unwrap_or_default();
        let repeat = !story.is_empty() && last_story.get(&game_id) == Some(&story);
        match tokens.get(move_idx + 1) {
            Some(Token::Comment(_)) => {
                if has_content {
                    tokens[move_idx + 1] = Token::Comment(prose);
                } else {
                    tokens.remove(move_idx + 1);
                }
            }
            _ => {
                if has_content && !repeat {
                    tokens.insert(move_idx + 1, Token::Comment(prose));
                }
            }
        }
        if has_content {
            last_story.insert(game_id, story);
        }
        crate::edit::update_game_tokens(conn, game_id, &tokens)?;
        conn.execute(
            "UPDATE jobs SET folded_at = datetime('now') WHERE id = ?1",
            [job_id],
        )?;
        report.folded += 1;
    }
    Ok(report)
}
