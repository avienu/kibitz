//! The per-position explanation builder (run 6, design/handoff-1).
//!
//! Turns a [`FeatureRecord`] into the [`Explanation`] object the game
//! view consumes: a verdict tag, an eval readout, a dual-voice headline,
//! and one block per narrated item — each block carrying the exact
//! evidence (squares + arrows) the board should show while that sentence
//! is read. The UI never synthesizes prose or evidence; everything comes
//! from here.
//!
//! Prose parity: block texts are produced by the SAME renderers the
//! narration pipeline uses, in both voices at once, so the voice toggle
//! swaps words but can never change the evidence.

use std::cmp::Reverse;
use std::collections::BTreeSet;

use kibitz_core::record::{
    ArrowKind, BlockKind, CompositePlan, EvalReadout, Evidence, EvidenceArrow, Explanation,
    ExplanationBlock, FeatureRecord, Imbalance, PlanHint, TacticAlert, VoiceText, SCHEMA_VERSION,
};

use crate::board::Board;
use crate::Voice;

/// Build the full explanation for one analyzed position.
pub fn explain(record: &FeatureRecord) -> Explanation {
    let board = Board::from_fen(&record.fen);

    let mut blocks: Vec<ExplanationBlock> = Vec::new();

    // --- Alert blocks, most severe first (same order as narration). ---
    let mut alerts: Vec<&TacticAlert> = record.wsui.alerts.iter().collect();
    alerts.sort_by_key(|a| Reverse(a.severity));
    // Clause dedupe runs per voice, mirroring verbalize_sections.
    let mut seen_coach: std::collections::HashSet<String> = Default::default();
    let mut seen_neutral: std::collections::HashSet<String> = Default::default();
    for alert in &alerts {
        let coach: Vec<String> = crate::render_alert_sentences(alert, &board, Voice::Coach)
            .into_iter()
            .filter(|s| seen_coach.insert(s.clone()))
            .collect();
        let neutral: Vec<String> = crate::render_alert_sentences(alert, &board, Voice::Neutral)
            .into_iter()
            .filter(|s| seen_neutral.insert(s.clone()))
            .collect();
        if coach.is_empty() && neutral.is_empty() {
            continue;
        }
        blocks.push(ExplanationBlock {
            kind: BlockKind::Alert,
            text: VoiceText {
                coach: coach.join(" "),
                neutral: neutral.join(" "),
            },
            evidence: alert_evidence(alert),
        });
    }

    // --- Imbalance blocks (dominance selection mirrors narration). ---
    for imbalance in crate::select_imbalances(&record.imbalances) {
        blocks.push(ExplanationBlock {
            kind: BlockKind::Imbalance,
            text: VoiceText {
                coach: crate::render_imbalance(imbalance, &board, record.phase, Voice::Coach),
                neutral: crate::render_imbalance(imbalance, &board, record.phase, Voice::Neutral),
            },
            evidence: imbalance_evidence(imbalance),
        });
    }

    // --- Plan blocks: composites first, leftover single hints after. ---
    let mut consumed: BTreeSet<&str> = BTreeSet::new();
    for (i, cp) in record.composite_plans.iter().take(2).enumerate() {
        if cp.supporting.len() < 2 {
            continue;
        }
        for h in &cp.hints {
            consumed.insert(h.as_str());
        }
        blocks.push(ExplanationBlock {
            kind: BlockKind::Plan,
            text: VoiceText {
                coach: crate::render_composite(cp, i, Voice::Coach),
                neutral: crate::render_composite(cp, i, Voice::Neutral),
            },
            evidence: composite_evidence(cp),
        });
    }
    let mut plan_index = blocks.iter().filter(|b| b.kind == BlockKind::Plan).count();
    for imbalance in crate::select_imbalances(&record.imbalances) {
        for plan in &imbalance.plans {
            if !consumed.insert(plan.hint.as_str()) {
                continue;
            }
            blocks.push(ExplanationBlock {
                kind: BlockKind::Plan,
                text: VoiceText {
                    coach: crate::render_plan(plan, imbalance.favors, plan_index, Voice::Coach),
                    neutral: crate::render_plan(plan, imbalance.favors, plan_index, Voice::Neutral),
                },
                evidence: hint_evidence(plan),
            });
            plan_index += 1;
        }
    }

    // --- Headline: the lead block's first sentence, removed from the
    // block so the panel never says it twice. ---
    let headline = match blocks.first_mut() {
        Some(lead) => VoiceText {
            coach: split_first_sentence(&mut lead.text.coach),
            neutral: split_first_sentence(&mut lead.text.neutral),
        },
        None => VoiceText {
            coach: crate::templates::lookup_voiced(Voice::Coach, &["empty"]).to_string(),
            neutral: crate::templates::lookup_voiced(Voice::Neutral, &["empty"]).to_string(),
        },
    };
    blocks.retain(|b| !b.text.coach.is_empty() || !b.text.neutral.is_empty());

    Explanation {
        schema_version: SCHEMA_VERSION,
        tag: tag_for(record).to_string(),
        eval: eval_readout(record),
        headline,
        blocks,
    }
}

fn tag_for(record: &FeatureRecord) -> &'static str {
    let mate_known = record.engine.as_ref().is_some_and(|e| e.mate_in.is_some())
        || record
            .wsui
            .alerts
            .iter()
            .any(|a| a.engine_check.as_ref().is_some_and(|c| c.mate_in.is_some()));
    if mate_known {
        "FORCED MATE"
    } else if record.wsui.screen_fired {
        "TACTICAL SCREEN FIRED"
    } else {
        "QUIET POSITION"
    }
}

/// White-POV readout: whole-position engine eval when present, else the
/// lead alert's confirmed verdict converted from beneficiary POV.
fn eval_readout(record: &FeatureRecord) -> Option<EvalReadout> {
    if let Some(engine) = &record.engine {
        if let Some(mate) = engine.mate_in {
            return Some(EvalReadout {
                cp: None,
                mate: Some(mate),
                display: format!("#{}", mate.abs()),
            });
        }
        return Some(EvalReadout {
            cp: Some(engine.eval_cp),
            mate: None,
            display: format!("{:+.1}", f64::from(engine.eval_cp) / 100.0),
        });
    }
    let lead = record.wsui.alerts.first()?;
    let check = lead.engine_check.as_ref()?;
    // The beneficiary is the side OPPOSITE the alert's owner.
    let white_benefits = lead.side == kibitz_core::record::SideColor::Black;
    let to_white = |v: i32| if white_benefits { v } else { -v };
    if let Some(mate) = check.mate_in {
        let m = to_white(mate);
        return Some(EvalReadout {
            cp: None,
            mate: Some(m),
            display: format!("#{}", m.abs()),
        });
    }
    let delta = check.score_delta_cp?;
    let cp = to_white(delta);
    Some(EvalReadout {
        cp: Some(cp),
        mate: None,
        display: format!("{:+.1}", f64::from(cp) / 100.0),
    })
}

fn alert_evidence(alert: &TacticAlert) -> Evidence {
    let mut ev = Evidence {
        alerts: alert.target.iter().cloned().collect(),
        attackers: alert.attackers.clone(),
        defenders: alert.defenders.clone(),
        ..Default::default()
    };
    if let Some(target) = &alert.target {
        for from in &alert.attackers {
            ev.arrows.push(EvidenceArrow {
                from: from.clone(),
                to: target.clone(),
                kind: ArrowKind::Attacker,
            });
        }
    }
    ev
}

/// Squares an imbalance shows: every square-shaped string anywhere in its
/// structured evidence values (the spec documents keys like
/// "isolated": ["d5"]). Plan-hint squares belong to plan blocks, not here.
fn imbalance_evidence(imbalance: &Imbalance) -> Evidence {
    let mut squares: Vec<String> = Vec::new();
    for value in imbalance.evidence.values() {
        collect_squares(value, &mut squares);
    }
    squares.dedup();
    Evidence {
        imbalance: squares,
        ..Default::default()
    }
}

fn collect_squares(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            for token in s.split(|c: char| !c.is_ascii_alphanumeric()) {
                if is_square(token) && !out.iter().any(|x| x == token) {
                    out.push(token.to_string());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_squares(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_squares(item, out);
            }
        }
        _ => {}
    }
}

fn is_square(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && (b'a'..=b'h').contains(&b[0]) && (b'1'..=b'8').contains(&b[1])
}

fn composite_evidence(cp: &CompositePlan) -> Evidence {
    let mut ev = Evidence {
        key: cp
            .squares
            .iter()
            .filter(|s| is_square(s))
            .cloned()
            .collect(),
        ..Default::default()
    };
    // Route arrows: any pair of consecutive squares in the composite that
    // ends on the target draws source → target.
    for pair in cp.squares.windows(2) {
        if is_square(&pair[0]) && pair[1] == cp.target {
            ev.arrows.push(EvidenceArrow {
                from: pair[0].clone(),
                to: pair[1].clone(),
                kind: ArrowKind::Key,
            });
        }
    }
    ev
}

fn hint_evidence(plan: &PlanHint) -> Evidence {
    let mut ev = Evidence {
        key: plan
            .squares
            .iter()
            .filter(|s| is_square(s))
            .cloned()
            .collect(),
        ..Default::default()
    };
    if plan.squares.len() >= 2 {
        let (from, to) = (&plan.squares[0], &plan.squares[plan.squares.len() - 1]);
        if is_square(from) && is_square(to) && from != to {
            ev.arrows.push(EvidenceArrow {
                from: from.clone(),
                to: to.clone(),
                kind: ArrowKind::Key,
            });
        }
    }
    ev
}

/// Remove and return the first sentence (". " boundary; falls back to the
/// whole string when there is only one sentence).
fn split_first_sentence(text: &mut String) -> String {
    match text.find(". ") {
        Some(idx) => {
            let head = text[..=idx].to_string();
            let rest = text[idx + 2..].to_string();
            *text = rest;
            head
        }
        None => std::mem::take(text),
    }
}
