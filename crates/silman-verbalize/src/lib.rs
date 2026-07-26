//! silman-verbalize: deterministic, template-driven prose from a
//! [`FeatureRecord`] (docs/SILMAN_ENGINE_SPEC.md, "silman-verbalize").
//!
//! Template mode is the default and is fully offline. Every user-visible
//! sentence skeleton lives in the data files under `templates/` (embedded
//! with `include_str!` and parsed once, at first use). This module only
//! selects templates and fills their slots with values taken verbatim from
//! the record — squares, SAN moves, evidence lists — or derived from the
//! record's own FEN (piece identity and color for a referenced square). By
//! construction the output never mentions a chess fact that is not present
//! in the record; that is the hard property the future LLM mode (the `llm`
//! feature) will be validated against.
//!
//! Composition order, per the spec: tactical alerts first (most severe
//! first), then dominant imbalances (winning > clear > minor, where minor
//! ones are dropped when the record carries three or more imbalances and at
//! least one stronger imbalance exists), then plans collected from the
//! rendered imbalances' hints, deduplicated by hint token.

mod board;
mod phrase;
mod templates;

use std::cmp::Reverse;
use std::collections::BTreeSet;

use serde_json::Value;
use silman_core::record::{
    AlertKind, EngineCheck, EngineCheckStatus, Favors, FeatureRecord, Imbalance, Magnitude, Phase,
    PlanHint, SideColor, TacticAlert,
};

use board::{Board, PieceKind};
use phrase::{
    capitalize_first, decapitalize_first, favors_side, humanize, join_and, magnitude_key,
    pawns_amount, phase_key, see_key, severity_key, side_name,
};
use templates::{fill, lookup, rotation};

/// The three prose sections of a verbalized position, in composition order.
/// Each section is a single paragraph; empty strings mean the record had
/// nothing to say for that section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sections {
    pub tactics: String,
    pub imbalances: String,
    pub plans: String,
}

/// Anything that can turn a [`FeatureRecord`] into prose. The optional LLM
/// implementation (feature `llm`, Phase 4) implements this same trait and
/// falls back to the template output when its post-validation fails.
pub trait Verbalizer {
    fn verbalize(&self, record: &FeatureRecord) -> String;
}

/// The default deterministic template verbalizer.
#[derive(Debug, Clone, Copy, Default)]
pub struct TemplateVerbalizer;

impl Verbalizer for TemplateVerbalizer {
    fn verbalize(&self, record: &FeatureRecord) -> String {
        verbalize(record)
    }
}

/// Render the whole record as coach-style prose: tactics, then imbalances,
/// then plans, as blank-line-separated paragraphs. A record with nothing to
/// report yields a single graceful "nothing stands out" line.
pub fn verbalize(record: &FeatureRecord) -> String {
    let sections = verbalize_sections(record);
    if sections.tactics.is_empty() && sections.imbalances.is_empty() && sections.plans.is_empty() {
        return lookup(&["empty"]).to_string();
    }
    let mut paragraphs: Vec<String> = Vec::new();
    if sections.tactics.is_empty() {
        paragraphs.push(lookup(&["tactics.quiet"]).to_string());
    } else {
        paragraphs.push(sections.tactics);
    }
    if !sections.imbalances.is_empty() {
        paragraphs.push(sections.imbalances);
    }
    if !sections.plans.is_empty() {
        paragraphs.push(sections.plans);
    }
    paragraphs.join("\n\n")
}

/// Render the record into its three sections without joining them, for
/// callers (the app UI) that lay the sections out separately.
pub fn verbalize_sections(record: &FeatureRecord) -> Sections {
    let board = Board::from_fen(&record.fen);

    // Tactical alerts, most severe first (stable within a severity).
    let mut alerts: Vec<&TacticAlert> = record.wsui.alerts.iter().collect();
    alerts.sort_by_key(|alert| Reverse(alert.severity));
    let tactics = alerts
        .iter()
        .enumerate()
        .map(|(index, alert)| apply_starter("rotation.alert", index, render_alert(alert, &board)))
        .collect::<Vec<_>>()
        .join(" ");

    // Dominant imbalances: winning > clear > minor; with three or more
    // imbalances the minor ones are noise unless nothing stronger exists.
    let mut ranked: Vec<&Imbalance> = record.imbalances.iter().collect();
    ranked.sort_by_key(|imbalance| Reverse(imbalance.magnitude));
    let drop_minor = record.imbalances.len() >= 3
        && ranked
            .iter()
            .any(|imbalance| imbalance.magnitude > Magnitude::Minor);
    let selected: Vec<&Imbalance> = if drop_minor {
        ranked
            .into_iter()
            .filter(|imbalance| imbalance.magnitude > Magnitude::Minor)
            .collect()
    } else {
        ranked
    };

    let mut imbalance_sentences: Vec<String> = selected
        .iter()
        .enumerate()
        .map(|(index, imbalance)| {
            apply_starter(
                "rotation.imbalance",
                index,
                render_imbalance(imbalance, &board, record.phase),
            )
        })
        .collect();
    if let Some(engine) = &record.engine {
        let score = format!("{:+.1}", f64::from(engine.eval_cp) / 100.0);
        imbalance_sentences.push(fill(
            lookup(&["engine.eval"]),
            &[("score", &score), ("best", &engine.best)],
        ));
    }
    let imbalances = imbalance_sentences.join(" ");

    // Plans from the rendered imbalances only, deduplicated by hint token.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut plan_sentences: Vec<String> = Vec::new();
    for imbalance in &selected {
        for plan in &imbalance.plans {
            if seen.insert(plan.hint.as_str()) {
                plan_sentences.push(render_plan(plan, imbalance.favors, plan_sentences.len()));
            }
        }
    }

    Sections {
        tactics,
        imbalances,
        plans: plan_sentences.join(" "),
    }
}

/// Prefix the paragraph with its rotation starter. Without a starter the
/// first letter is capitalized; with one, the sentence is decapitalized
/// unless it opens with a side name (a proper noun).
fn apply_starter(rotation_key: &str, index: usize, paragraph: String) -> String {
    let starter = rotation(rotation_key, index);
    if starter.is_empty() {
        return capitalize_first(&paragraph);
    }
    let white = lookup(&["side.white"]);
    let black = lookup(&["side.black"]);
    let body = if paragraph.starts_with(white) || paragraph.starts_with(black) {
        paragraph
    } else {
        decapitalize_first(&paragraph)
    };
    format!("{starter} {body}")
}

fn render_alert(alert: &TacticAlert, board: &Board) -> String {
    let side = side_name(alert.side);
    let subject = match (alert.kind, alert.target.as_deref()) {
        (AlertKind::WeakKing, _) => fill(lookup(&["phrase.kings"]), &[("side", side)]),
        (_, Some(target)) => piece_subject(board, target, side),
        (_, None) => fill(lookup(&["phrase.some_piece"]), &[("side", side)]),
    };
    let target_clause = match (alert.kind, alert.target.as_deref()) {
        (AlertKind::WeakKing, Some(target)) => format!(
            " {}",
            fill(lookup(&["clause.weak_king_target"]), &[("target", target)])
        ),
        _ => String::new(),
    };
    let kind = format!("{:?}", alert.kind);
    let lead_key = format!("alert.{kind}.{}", severity_key(alert.severity));
    let mut sentences = vec![fill(
        lookup(&[lead_key.as_str(), "alert.generic"]),
        &[("subject", &subject), ("target_clause", &target_clause)],
    )];

    let attackers = owned_list(board, &alert.attackers);
    let defenders = owned_list(board, &alert.defenders);
    if alert.kind == AlertKind::Undefended {
        // The lead sentence already states that nothing defends it.
        if !attackers.is_empty() {
            sentences.push(fill(
                lookup(&["clause.attacked_by"]),
                &[("attackers", &attackers)],
            ));
        }
    } else {
        match (attackers.is_empty(), defenders.is_empty()) {
            (false, false) => sentences.push(fill(
                lookup(&["clause.attack_defend.both"]),
                &[("attackers", &attackers), ("defenders", &defenders)],
            )),
            (false, true) => sentences.push(fill(
                lookup(&["clause.attack_defend.attackers"]),
                &[("attackers", &attackers)],
            )),
            (true, false) => sentences.push(fill(
                lookup(&["clause.attack_defend.defenders"]),
                &[("defenders", &defenders)],
            )),
            (true, true) => {}
        }
    }

    if let Some(detail) = alert.detail.as_deref() {
        let detail_key = format!("detail.{detail}");
        let known = lookup(&[detail_key.as_str()]);
        let detail_phrase = if known.is_empty() {
            humanize(detail)
        } else {
            known.to_string()
        };
        sentences.push(fill(
            lookup(&["clause.detail"]),
            &[("detail", &detail_phrase)],
        ));
    }

    if let Some(see) = alert.see {
        if see > 0 {
            sentences.push(fill(
                lookup(&["clause.see"]),
                &[("amount", lookup(&[see_key(see)]))],
            ));
        }
    }

    if let Some(check) = &alert.engine_check {
        sentences.push(render_engine_check(check));
    }

    sentences.join(" ")
}

fn render_engine_check(check: &EngineCheck) -> String {
    match check.status {
        EngineCheckStatus::Confirmed => {
            let pv = check.pv.join(" ");
            match (check.pv.is_empty(), check.score_delta_cp) {
                (false, Some(delta)) => fill(
                    lookup(&["engine.confirmed.pv_delta"]),
                    &[("pv", &pv), ("delta", &pawns_amount(delta))],
                ),
                (false, None) => fill(lookup(&["engine.confirmed.pv"]), &[("pv", &pv)]),
                (true, Some(delta)) => fill(
                    lookup(&["engine.confirmed.delta"]),
                    &[("delta", &pawns_amount(delta))],
                ),
                (true, None) => lookup(&["engine.confirmed"]).to_string(),
            }
        }
        EngineCheckStatus::Refuted => lookup(&["engine.refuted"]).to_string(),
        EngineCheckStatus::UnclearAtBudget => lookup(&["engine.unclear"]).to_string(),
    }
}

fn render_imbalance(imbalance: &Imbalance, board: &Board, phase: Phase) -> String {
    let kind = format!("{:?}", imbalance.kind);
    let order = lookup(&["evidence.order"]);
    let rank = |key: &str| {
        order
            .split('|')
            .position(|known| known == key)
            .unwrap_or(usize::MAX)
    };
    let mut entries: Vec<(&String, &Value)> = imbalance.evidence.iter().collect();
    entries.sort_by_key(|(key, _)| rank(key));
    let evidence: Vec<String> = entries
        .iter()
        .filter_map(|(key, value)| render_evidence(key, value, board))
        .collect();
    let evidence_clause = if evidence.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            fill(
                lookup(&["clause.evidence"]),
                &[("list", &join_and(&evidence))],
            )
        )
    };
    let aspect_key = format!("aspect.{kind}");
    let known_aspect = lookup(&[aspect_key.as_str()]);
    let aspect = if known_aspect.is_empty() {
        humanize(&kind)
    } else {
        known_aspect.to_string()
    };

    match favors_side(imbalance.favors) {
        None => {
            let balanced_key = format!("imbalance.{kind}.balanced");
            fill(
                lookup(&[balanced_key.as_str(), "imbalance.balanced"]),
                &[("aspect", &aspect), ("evidence_clause", &evidence_clause)],
            )
        }
        Some(side) => {
            let magnitude = magnitude_key(imbalance.magnitude);
            let phased_key = format!("imbalance.{kind}.{magnitude}.{}", phase_key(phase));
            let plain_key = format!("imbalance.{kind}.{magnitude}");
            let generic_key = format!("imbalance.generic.{magnitude}");
            fill(
                lookup(&[
                    phased_key.as_str(),
                    plain_key.as_str(),
                    generic_key.as_str(),
                ]),
                &[
                    ("beneficiary", side_name(side)),
                    ("aspect", &aspect),
                    ("evidence_clause", &evidence_clause),
                ],
            )
        }
    }
}

/// Evidence keys whose values are squares of the owner's pawns; for these
/// the record's FEN is consulted so the phrase can say whose pawn it is.
const PAWN_EVIDENCE_KEYS: &[&str] = &["isolated", "doubled", "backward", "passed", "hanging"];

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn render_evidence(key: &str, value: &Value, board: &Board) -> Option<String> {
    let items: Vec<String> = match value {
        Value::Null | Value::Bool(false) => return None,
        Value::Bool(true) => Vec::new(),
        Value::Array(values) => values.iter().map(scalar).collect(),
        Value::Object(map) => map
            .iter()
            .map(|(name, inner)| format!("{name} {}", scalar(inner)))
            .collect(),
        other => vec![scalar(other)],
    };
    let list = join_and(&items);
    let plural = items.len() > 1;
    let owner = if PAWN_EVIDENCE_KEYS.contains(&key) {
        pawn_owner(board, &items)
    } else {
        None
    };

    let mut chain: Vec<String> = Vec::new();
    if owner.is_some() {
        if plural {
            chain.push(format!("evidence.{key}.owned.plural"));
        }
        chain.push(format!("evidence.{key}.owned"));
    }
    if plural {
        chain.push(format!("evidence.{key}.plural"));
    }
    chain.push(format!("evidence.{key}"));
    chain.push(if items.is_empty() {
        "evidence.generic.bare".to_string()
    } else {
        "evidence.generic".to_string()
    });
    let keys: Vec<&str> = chain.iter().map(String::as_str).collect();
    let template = lookup(&keys);

    let owner_name = owner.map(side_name).unwrap_or("");
    let human_key = humanize(key);
    Some(fill(
        template,
        &[("items", &list), ("owner", owner_name), ("key", &human_key)],
    ))
}

/// `Some(side)` when every listed square holds a pawn of one color on the
/// record's FEN; `None` otherwise (the neutral phrasing is used instead).
fn pawn_owner(board: &Board, squares: &[String]) -> Option<SideColor> {
    let mut owner: Option<SideColor> = None;
    for square in squares {
        match board.piece_at(square) {
            Some((color, PieceKind::Pawn)) => match owner {
                None => owner = Some(color),
                Some(existing) if existing == color => {}
                Some(_) => return None,
            },
            _ => return None,
        }
    }
    owner
}

fn render_plan(plan: &PlanHint, favors: Favors, index: usize) -> String {
    let hint_key = format!("plan.{}", plan.hint);
    let known = lookup(&[hint_key.as_str()]);
    let action = if known.is_empty() {
        humanize(&plan.hint)
    } else {
        known.to_string()
    };
    let squares_clause = if plan.squares.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            fill(
                lookup(&["clause.plan_squares"]),
                &[("squares", &join_and(&plan.squares))],
            )
        )
    };
    let lead_key = match (favors_side(favors), index) {
        (Some(_), 0) => "plan.lead.side",
        (Some(_), _) => "plan.lead.side.more",
        (None, 0) => "plan.lead.neutral",
        (None, _) => "plan.lead.neutral.more",
    };
    let side = favors_side(favors).map(side_name).unwrap_or("");
    fill(
        lookup(&[lead_key]),
        &[
            ("side", side),
            ("plan", &action),
            ("squares_clause", &squares_clause),
        ],
    )
}

/// "White's knight on e5", or a neutral fallback when the FEN does not show
/// a piece on that square (the verbalizer never guesses piece identity).
fn piece_subject(board: &Board, square: &str, alert_side: &str) -> String {
    match board.piece_at(square) {
        Some((color, kind)) => fill(
            lookup(&["phrase.owned_piece"]),
            &[
                ("side", side_name(color)),
                ("piece", lookup(&[kind.template_key()])),
                ("square", square),
            ],
        ),
        None => fill(
            lookup(&["phrase.side_piece"]),
            &[("side", alert_side), ("square", square)],
        ),
    }
}

/// Phrase a square list with owners from the FEN. When every square resolves
/// to one color, only the first phrase names the side ("White's knight on e5
/// and the bishop on b5"); unresolved squares fall back to neutral phrasing.
fn owned_list(board: &Board, squares: &[String]) -> String {
    if squares.is_empty() {
        return String::new();
    }
    let pieces: Vec<Option<(SideColor, PieceKind)>> = squares
        .iter()
        .map(|square| board.piece_at(square))
        .collect();
    let colors: Vec<SideColor> = pieces.iter().flatten().map(|(color, _)| *color).collect();
    let uniform = colors.len() == pieces.len() && colors.windows(2).all(|pair| pair[0] == pair[1]);

    let phrases: Vec<String> = squares
        .iter()
        .zip(&pieces)
        .enumerate()
        .map(|(index, (square, piece))| match piece {
            Some((color, kind)) => {
                let key = if uniform && index > 0 {
                    "phrase.bare_piece"
                } else {
                    "phrase.owned_piece"
                };
                fill(
                    lookup(&[key]),
                    &[
                        ("side", side_name(*color)),
                        ("piece", lookup(&[kind.template_key()])),
                        ("square", square),
                    ],
                )
            }
            None => fill(lookup(&["phrase.unknown_piece"]), &[("square", square)]),
        })
        .collect();
    join_and(&phrases)
}
