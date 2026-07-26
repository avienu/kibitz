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
                let record = silman_core::analyze(&board);
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
                let summary: String = record
                    .imbalances
                    .iter()
                    .filter(|i| i.magnitude >= Magnitude::Clear)
                    .map(|i| format!("{:?}:{:?};", i.kind, i.favors))
                    .collect();
                let notable = fired || (!summary.is_empty() && summary != last_summary);
                if notable && report.comments_added < max_comments {
                    let prose = silman_verbalize::verbalize(&record);
                    // Compress paragraphs for an inline comment.
                    let text = prose.replace("\n\n", " ");
                    inserts.push((idx + 1, text));
                    report.comments_added += 1;
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
