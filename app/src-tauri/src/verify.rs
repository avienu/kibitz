//! `verify_suggestions` IPC (run 11): the cursory engine review behind
//! the CONSIDER chips.
//!
//! The static explanation renders instantly (explain.rs, no engine);
//! the frontend then calls this ONLY when the explanation's WSUI screen
//! fired and suggestions exist. The maintainer's ruling sanctions
//! exactly that trigger ("at least a cursory engine review, at least if
//! tactics screen is present") — and this command re-checks the gate
//! server-side: a quiet position returns `ran: false` without touching
//! the engine (CLAUDE.md #6).
//!
//! Work is bounded: one baseline search of the position plus one
//! `go nodes` search per candidate (kibitz-core caps suggestions at 3,
//! so at most 4 searches of [`kibitz_db::verify::VERIFY_NODES`] nodes).
//! The response is FEN-stamped like `engine-info` events (audit 2026-07
//! #5): the frontend must drop results whose stamp no longer matches
//! the position it is showing.

use serde::Serialize;
use tauri::State;

use crate::uci::{self, UciPosition};
use crate::EngineState;

/// One reviewed candidate: `verdict` is `"cleared"` or `"refuted"`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedSuggestion {
    pub uci: String,
    pub san: String,
    pub verdict: kibitz_db::verify::Verdict,
}

/// `verify_suggestions` response, FEN-stamped for staleness checks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOut {
    /// The FEN the review ran on — attribute results to THIS position,
    /// never to "whatever is currently shown".
    pub fen: String,
    /// False when the WSUI screen did not fire or nothing was suggested:
    /// the engine was NOT touched.
    pub ran: bool,
    pub verdicts: Vec<VerifiedSuggestion>,
    /// Node budget of each bounded search (0 when `ran` is false).
    pub nodes_per_search: u64,
}

/// The engine-off gate, separated for offline testing: `Some` only when
/// the position's WSUI screen fired AND the static suggester proposed
/// moves — the only condition under which the engine may run here.
pub(crate) fn static_gate(
    fen: &str,
) -> Result<Option<(cozy_chess::Board, Vec<kibitz_core::suggest::Suggestion>)>, String> {
    let board: cozy_chess::Board = fen.parse().map_err(|e| format!("bad FEN {fen:?}: {e:?}"))?;
    let record = kibitz_core::analyze(&board);
    if !record.wsui.screen_fired {
        return Ok(None);
    }
    let suggestions = kibitz_core::suggest::suggest(&record, &board);
    if suggestions.is_empty() {
        return Ok(None);
    }
    Ok(Some((board, suggestions)))
}

/// One bounded search of `fen`, returning the final side-to-move-POV
/// score with mate folded into the ±10000 sentinel.
async fn bounded_score(engine: &mut uci::Engine, fen: &str, nodes: u64) -> Result<i32, String> {
    let mut last: Option<i32> = None;
    engine
        .analyze(&UciPosition::Fen(fen.to_string()), Some(nodes), |info| {
            if let Some(score) = kibitz_db::verify::fold_score(info.score_cp, info.score_mate) {
                last = Some(score);
            }
        })
        .await?;
    last.ok_or_else(|| "engine produced no score".to_string())
}

/// Run the cursory review for `fen`. Verification is a follow-up to the
/// static explanation — the frontend renders chips first, then calls
/// this; refuted chips disappear, cleared marked chips appear.
#[tauri::command]
pub async fn verify_suggestions(
    state: State<'_, EngineState>,
    fen: String,
    user_path: Option<String>,
) -> Result<VerifyOut, String> {
    let Some((board, suggestions)) = static_gate(&fen)? else {
        return Ok(VerifyOut {
            fen,
            ran: false,
            verdicts: Vec::new(),
            nodes_per_search: 0,
        });
    };
    let nodes = kibitz_db::verify::VERIFY_NODES;
    let path = uci::resolve_engine_path(user_path.as_deref())?;

    // Same lazy-spawn slot as analyze_position: holding the lock for the
    // whole (bounded) review serializes it with live analysis.
    let mut slot = state.engine.lock().await;
    let result = async {
        crate::ensure_engine(&mut slot, &state.stop, &path).await?;
        let engine = slot.as_mut().expect("engine just ensured");
        let baseline = bounded_score(engine, &fen, nodes).await?;
        let mut cands = Vec::with_capacity(suggestions.len());
        for s in &suggestions {
            let score = match s.mv.parse::<cozy_chess::Move>() {
                Ok(mv) if board.is_legal(mv) => {
                    let mut b2 = board.clone();
                    b2.play(mv);
                    // The child search reports the OPPONENT's POV.
                    bounded_score(engine, &b2.to_string(), nodes)
                        .await
                        .ok()
                        .map(|cp| -cp)
                }
                _ => None,
            };
            cands.push(kibitz_db::verify::CandidateEval {
                uci: s.mv.clone(),
                static_risk: s.static_risk,
                score,
            });
        }
        Ok::<_, String>(kibitz_db::verify::decide(baseline, &cands))
    }
    .await;
    let decisions = match result {
        Ok(decisions) => decisions,
        Err(e) => {
            // Engine likely died; drop it so the next run respawns.
            *slot = None;
            *state.stop.lock().expect("stop mutex poisoned") = None;
            return Err(e);
        }
    };

    let verdicts = suggestions
        .iter()
        .zip(&decisions)
        .map(|(s, (_, verdict))| VerifiedSuggestion {
            uci: s.mv.clone(),
            san: s.san.clone(),
            verdict: *verdict,
        })
        .collect();
    Ok(VerifyOut {
        fen,
        ran: true,
        verdicts,
        nodes_per_search: nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine-off gate: a quiet position may NEVER reach the engine
    /// from here — only a fired screen with suggestions passes.
    #[test]
    fn static_gate_blocks_quiet_positions() {
        // Startpos: no screen, no engine.
        assert!(
            static_gate("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .unwrap()
                .is_none()
        );
        // French Winawer after 5.a3 (the field report): screen fired
        // (the b4-bishop hangs) and the static suggester proposes moves
        // — all statically marked, awaiting this review.
        let (_, suggestions) =
            static_gate("rnbqk1nr/pp3ppp/4p3/2ppP3/1b1P4/P1N5/1PP2PPP/R1BQKBNR b KQkq - 0 5")
                .unwrap()
                .expect("screen fired with suggestions");
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().all(|s| s.static_risk.is_some()));
        // Garbage FEN is an error, not a panic.
        assert!(static_gate("not a fen").is_err());
    }

    /// Offline IPC shape: camelCase field names and lowercase verdicts —
    /// the TypeScript contract.
    #[test]
    fn verify_out_serializes_the_ts_contract() {
        let out = VerifyOut {
            fen: "8/8/8/8/8/8/8/K1k5 w - - 0 1".into(),
            ran: true,
            verdicts: vec![
                VerifiedSuggestion {
                    uci: "c5d4".into(),
                    san: "cxd4".into(),
                    verdict: kibitz_db::verify::Verdict::Cleared,
                },
                VerifiedSuggestion {
                    uci: "f7f5".into(),
                    san: "f5".into(),
                    verdict: kibitz_db::verify::Verdict::Refuted,
                },
            ],
            nodes_per_search: kibitz_db::verify::VERIFY_NODES,
        };
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["ran"], true);
        assert_eq!(v["nodesPerSearch"], kibitz_db::verify::VERIFY_NODES);
        assert_eq!(v["verdicts"][0]["uci"], "c5d4");
        assert_eq!(v["verdicts"][0]["verdict"], "cleared");
        assert_eq!(v["verdicts"][1]["verdict"], "refuted");
        assert!(v["fen"].is_string());
    }
}
