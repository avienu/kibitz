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
//! The prose is prose: no serialized data may leak through. Every known
//! evidence key has a dedicated grammatical rendering; unknown evidence
//! keys are omitted entirely; bare counts (space, development, forcing
//! moves, locked pawns) are folded into qualitative phrasing or dropped;
//! centipawn figures are spoken as pawns. The snapshot tests enforce this
//! with a lint over the rendered output (no underscores, no brackets, no
//! braces, no quotes, no labeled numbers).
//!
//! Composition order, per the spec: tactical alerts first (most severe
//! first), then dominant imbalances (winning > clear > minor, where minor
//! ones are dropped when the record carries three or more imbalances and at
//! least one stronger imbalance exists), then plans collected from the
//! rendered imbalances' hints, deduplicated by hint token.

mod board;
#[cfg(feature = "llm")]
pub mod llm;
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
use templates::{fill, lookup, lookup_var, rotation};

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
    let mut seen_clauses: std::collections::HashSet<String> = Default::default();
    let tactics = alerts
        .iter()
        .enumerate()
        .map(|(index, alert)| {
            // Two alerts often share evidence (the same attackers pressing
            // the same square); state each clause once per comment.
            let sentences: Vec<String> = render_alert_sentences(alert, &board)
                .into_iter()
                .filter(|sentence| seen_clauses.insert(sentence.clone()))
                .collect();
            apply_starter("rotation.alert", index, sentences.join(" "))
        })
        .filter(|s| !s.is_empty())
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
        if let Some(mate) = engine.mate_in {
            let side = side_name(if mate >= 0 {
                SideColor::White
            } else {
                SideColor::Black
            });
            imbalance_sentences.push(fill(
                lookup(&["engine.eval.mate"]),
                &[
                    ("mate", &mate.abs().to_string()),
                    ("side", side),
                    ("best", &engine.best),
                ],
            ));
        } else {
            let score = format!("{:+.1}", f64::from(engine.eval_cp) / 100.0);
            imbalance_sentences.push(fill(
                lookup(&["engine.eval"]),
                &[("score", &score), ("best", &engine.best)],
            ));
        }
    }
    let imbalances = imbalance_sentences.join(" ");

    // Plans: composite plans (schema v2) lead when present — the top
    // convergence as a unified sentence, the runner-up briefly, the rest
    // dropped. Hints already consumed by a narrated composite are not
    // repeated as singles.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut plan_sentences: Vec<String> = Vec::new();
    for (i, cp) in record.composite_plans.iter().take(2).enumerate() {
        if cp.supporting.len() < 2 {
            continue;
        }
        for h in &cp.hints {
            seen.insert(h.as_str());
        }
        if i == 0 {
            let clauses: Vec<String> = cp
                .hints
                .iter()
                .filter_map(|h| {
                    templates::try_lookup(&format!("plan.composite.clause.{h}")).map(str::to_string)
                })
                .collect();
            let clause_text = if clauses.is_empty() {
                humanize(&cp.hints.join(", "))
            } else {
                join_and(&clauses)
            };
            plan_sentences.push(fill(
                lookup(&["plan.composite.lead"]),
                &[("target", &cp.target), ("clauses", &clause_text)],
            ));
        } else {
            plan_sentences.push(fill(
                lookup(&["plan.composite.runner_up"]),
                &[
                    ("target", &cp.target),
                    ("side", side_name_favors(cp.favors)),
                ],
            ));
        }
    }
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

fn render_alert_sentences(alert: &TacticAlert, board: &Board) -> Vec<String> {
    let side = side_name(alert.side);
    // Phrasing-variety seed: the target square's index, so the same alert
    // on the same square always reads identically, while adjacent alerts
    // of the same kind on different squares phrase differently.
    let seed = alert
        .target
        .as_deref()
        .and_then(|t| {
            let mut bytes = t.bytes();
            let file = bytes.next()?.checked_sub(b'a')?;
            let rank = bytes.next()?.checked_sub(b'1')?;
            Some(file as usize * 8 + rank as usize)
        })
        .unwrap_or(0);
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
    let attackers = owned_list(board, &alert.attackers);
    let defenders = owned_list(board, &alert.defenders);
    let detail = alert.detail.as_deref();

    // A pure overload alert names the overworked defender itself: the
    // `defenders` field carries the pieces it is holding together.
    if detail == Some("overloaded-defender")
        && alert.attackers.is_empty()
        && !alert.defenders.is_empty()
    {
        return vec![fill(
            lookup(&["alert.overloaded"]),
            &[("subject", &subject), ("defenders", &defenders)],
        )];
    }

    // Lead sentence: a detail-specific lead absorbs the detail qualifier;
    // otherwise fall back to the severity lead and phrase the detail as a
    // follow-on sentence below.
    let kind = format!("{:?}", alert.kind);
    let detail_lead_key = detail.map(|token| format!("alert.{kind}.{token}"));
    let detail_in_lead = detail_lead_key
        .as_deref()
        .is_some_and(|key| !lookup(&[key]).is_empty());
    let severity_lead_key = format!("alert.{kind}.{}", severity_key(alert.severity));
    let mut lead_keys: Vec<&str> = Vec::new();
    if let Some(key) = detail_lead_key.as_deref() {
        lead_keys.push(key);
    }
    lead_keys.push(severity_lead_key.as_str());
    lead_keys.push("alert.generic");
    let mut sentences = vec![fill(
        lookup_var(&lead_keys, seed),
        &[("subject", &subject), ("target_clause", &target_clause)],
    )];

    if alert.kind == AlertKind::WeakKing {
        if !attackers.is_empty() {
            sentences.push(fill(
                lookup(&["clause.king_attackers"]),
                &[("attackers", &attackers)],
            ));
        }
    } else if alert.kind == AlertKind::Undefended {
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
                lookup_var(&["clause.attack_defend.both"], seed),
                &[("attackers", &attackers), ("defenders", &defenders)],
            )),
            (false, true) => sentences.push(fill(
                lookup(&["clause.attack_defend.attackers"]),
                &[("attackers", &attackers)],
            )),
            (true, false) => {
                let key = if alert.defenders.len() > 1 {
                    "clause.attack_defend.defenders.plural"
                } else {
                    "clause.attack_defend.defenders"
                };
                sentences.push(fill(lookup(&[key]), &[("defenders", &defenders)]));
            }
            (true, true) => {}
        }
    }

    if let Some(token) = detail {
        if !detail_in_lead {
            if alert.kind == AlertKind::WeakKing {
                sentences.push(render_king_details(token));
            } else {
                let known = lookup(&[&format!("detail.{token}")]);
                if known.is_empty() {
                    sentences.push(fill(
                        lookup(&["clause.detail"]),
                        &[("detail", &humanize(token))],
                    ));
                } else {
                    sentences.push(known.to_string());
                }
            }
        }
    }

    if let Some(see) = alert.see {
        if see > 0 {
            sentences.push(fill(
                lookup_var(&["clause.see"], seed),
                &[("amount", lookup(&[see_key(see)]))],
            ));
        }
    }

    if let Some(check) = &alert.engine_check {
        sentences.push(render_engine_check(check));
    }

    sentences
}

/// WeakKing `detail` is a semicolon-joined compound ("zone-pressure+3;
/// g-file shield pawn missing; open-files:g; back-rank"). Each component is
/// rewritten as a natural clause and the clauses joined into one sentence;
/// the zone-pressure count is folded away, and a shield file already
/// reported as wide open is not also reported as missing its pawn.
fn render_king_details(detail: &str) -> String {
    let mut zone_pressure = false;
    let mut back_rank = false;
    let mut missing: Vec<String> = Vec::new();
    let mut advanced: Vec<String> = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut extras: Vec<String> = Vec::new();
    for part in detail.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        if part.starts_with("zone-pressure") {
            zone_pressure = true;
        } else if part == "back-rank" {
            back_rank = true;
        } else if let Some(files) = part.strip_prefix("open-files:") {
            open.extend(
                files
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_string),
            );
        } else if let Some(file) = part.strip_suffix("-file shield pawn missing") {
            missing.push(file.to_string());
        } else if let Some(file) = part.strip_suffix("-file shield pawn advanced") {
            advanced.push(file.to_string());
        } else {
            extras.push(humanize(part));
        }
    }
    missing.retain(|file| !open.contains(file));

    let mut clauses: Vec<String> = Vec::new();
    if zone_pressure {
        clauses.push(lookup(&["king.zone_pressure"]).to_string());
    }
    if !missing.is_empty() {
        clauses.push(fill(
            lookup(&["king.shield_missing"]),
            &[("files", &file_phrase(&missing))],
        ));
    }
    if !advanced.is_empty() {
        let key = if advanced.len() > 1 {
            "king.shield_advanced.plural"
        } else {
            "king.shield_advanced"
        };
        clauses.push(fill(lookup(&[key]), &[("files", &file_phrase(&advanced))]));
    }
    if !open.is_empty() {
        let key = if open.len() > 1 {
            "king.open_files.plural"
        } else {
            "king.open_files"
        };
        clauses.push(fill(lookup(&[key]), &[("files", &file_phrase(&open))]));
    }
    if back_rank {
        clauses.push(lookup(&["king.back_rank"]).to_string());
    }
    clauses.extend(extras);
    format!("{}.", capitalize_first(&join_and(&clauses)))
}

fn side_name_favors(favors: Favors) -> &'static str {
    match favors_side(favors) {
        Some(side) => side_name(side),
        None => "both sides",
    }
}

fn render_engine_check(check: &EngineCheck) -> String {
    match check.status {
        EngineCheckStatus::Confirmed => {
            let pv = check.pv.join(" ");
            // Mate distances take absolute priority over material units
            // (run-5 bug 1): a mate score must never read as pawns.
            if let Some(mate) = check.mate_in {
                let m = mate.abs().to_string();
                return if mate == 0 {
                    fill(lookup(&["engine.confirmed.pv_mate_now"]), &[("pv", &pv)])
                } else if mate < 0 {
                    fill(lookup(&["engine.confirmed.mate_against"]), &[("mate", &m)])
                } else if pv.is_empty() {
                    fill(lookup(&["engine.confirmed.mate"]), &[("mate", &m)])
                } else {
                    fill(
                        lookup(&["engine.confirmed.pv_mate"]),
                        &[("pv", &pv), ("mate", &m)],
                    )
                };
            }
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

    // Cross-key context: "open" flavors the bishop-pair phrase, and a named
    // material pattern supersedes the raw centipawn figure.
    let context = EvidenceContext {
        open_position: imbalance.evidence.get("character").and_then(Value::as_str) == Some("open"),
        has_pattern: imbalance.evidence.contains_key("pattern"),
    };

    let order = lookup(&["evidence.order"]);
    let rank = |key: &str| {
        order
            .split('|')
            .position(|known| known == evidence_base_key(key))
            .unwrap_or(usize::MAX)
    };
    let mut entries: Vec<(&String, &Value)> = imbalance.evidence.iter().collect();
    entries.sort_by_key(|(key, _)| rank(key));
    let evidence: Vec<String> = entries
        .iter()
        .filter_map(|(key, value)| render_evidence(key, value, board, &context))
        .map(|clause| format!("{}.", capitalize_first(&clause)))
        .collect();

    let aspect_key = format!("aspect.{kind}");
    let known_aspect = lookup(&[aspect_key.as_str()]);
    let aspect = if known_aspect.is_empty() {
        humanize(&kind)
    } else {
        known_aspect.to_string()
    };

    let headline = match favors_side(imbalance.favors) {
        None => {
            let balanced_key = format!("imbalance.{kind}.balanced");
            fill(
                lookup(&[balanced_key.as_str(), "imbalance.balanced"]),
                &[("aspect", &aspect)],
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
                &[("beneficiary", side_name(side)), ("aspect", &aspect)],
            )
        }
    };
    if evidence.is_empty() {
        headline
    } else {
        format!("{headline} {}", evidence.join(" "))
    }
}

/// Cross-key evidence context within a single imbalance.
struct EvidenceContext {
    open_position: bool,
    has_pattern: bool,
}

/// The evidence key with any side/file suffix stripped, for ordering and
/// dispatch ("isolated_black" -> "isolated", "doubled_majors_d" ->
/// "doubled_majors", "holes_in_black_camp" -> "holes").
fn evidence_base_key(key: &str) -> &str {
    if key.starts_with("doubled_majors_") {
        return "doubled_majors";
    }
    if key == "holes_in_white_camp" || key == "holes_in_black_camp" {
        return "holes";
    }
    key.strip_suffix("_white")
        .or_else(|| key.strip_suffix("_black"))
        .unwrap_or(key)
}

/// The side named by an evidence key suffix, if any.
fn evidence_key_side(key: &str) -> Option<SideColor> {
    if key == "holes_in_white_camp" || key.ends_with("_white") {
        Some(SideColor::White)
    } else if key == "holes_in_black_camp" || key.ends_with("_black") {
        Some(SideColor::Black)
    } else {
        None
    }
}

fn side_from_value(value: &Value) -> Option<SideColor> {
    match value.as_str() {
        Some("white") => Some(SideColor::White),
        Some("black") => Some(SideColor::Black),
        _ => None,
    }
}

/// A non-empty list of strings from an array value; `None` otherwise.
/// Degenerate evidence (empty arrays, non-arrays) is thereby suppressed.
fn string_list(value: &Value) -> Option<Vec<String>> {
    let items: Vec<String> = value
        .as_array()?
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// "the d-file" / "the d- and e-files".
fn file_phrase(files: &[String]) -> String {
    if files.len() == 1 {
        fill(lookup(&["phrase.file"]), &[("items", &files[0])])
    } else {
        let dashed: Vec<String> = files.iter().map(|file| format!("{file}-")).collect();
        fill(lookup(&["phrase.files"]), &[("items", &join_and(&dashed))])
    }
}

/// One grammatical clause for a known evidence key, or `None` for keys that
/// are suppressed (bare counts, degenerate values) or unknown. Unknown keys
/// are omitted silently: serialized data must never leak into the prose.
fn render_evidence(
    key: &str,
    value: &Value,
    board: &Board,
    context: &EvidenceContext,
) -> Option<String> {
    // Doubled majors carry the file in the key ("doubled_majors_d") and the
    // owning side in the value.
    if let Some(file) = key.strip_prefix("doubled_majors_") {
        let side = side_from_value(value)?;
        return Some(fill(
            lookup(&["evidence.doubled_majors"]),
            &[
                ("side", side_name(side)),
                ("files", &file_phrase(&[file.to_string()])),
            ],
        ));
    }

    let base = evidence_base_key(key);
    let key_side = evidence_key_side(key);
    match base {
        // Bare counts: folded into the headline or the character clause,
        // never printed as labeled numbers.
        "locked_center_pawns"
        | "white_space"
        | "black_space"
        | "white_developed"
        | "black_developed"
        | "white_forcing_moves"
        | "black_forcing_moves" => None,

        "character" => match value.as_str() {
            // "open" flavors the bishop-pair clause instead (see below);
            // on its own it is the unmarked case and stays unspoken.
            Some("closed") => Some(lookup(&["evidence.character.closed"]).to_string()),
            _ => None,
        },

        "bishop_pair" => {
            let side = side_from_value(value).or(key_side)?;
            let template_key = if context.open_position {
                "evidence.bishop_pair.open"
            } else {
                "evidence.bishop_pair"
            };
            Some(fill(lookup(&[template_key]), &[("side", side_name(side))]))
        }

        "bad_bishop" => {
            let side = key_side?;
            let object = value.as_object()?;
            let square = object.get("bishop")?.as_str()?;
            let pawns = object
                .get("blocking_pawns")
                .and_then(string_list)
                .unwrap_or_default();
            if pawns.is_empty() {
                Some(fill(
                    lookup(&["evidence.bad_bishop.bare"]),
                    &[("side", side_name(side)), ("square", square)],
                ))
            } else {
                Some(fill(
                    lookup(&["evidence.bad_bishop"]),
                    &[
                        ("side", side_name(side)),
                        ("square", square),
                        ("pawns", &join_and(&pawns)),
                    ],
                ))
            }
        }

        "isolated" | "doubled" | "backward" | "passed" | "hanging" => {
            let squares = string_list(value)?;
            let side = key_side.or_else(|| pawn_owner(board, &squares));
            let plural = squares.len() > 1;
            let template_key = match (side.is_some(), plural) {
                (true, true) => format!("evidence.{base}.plural"),
                (true, false) => format!("evidence.{base}"),
                (false, true) => format!("evidence.{base}.neutral.plural"),
                (false, false) => format!("evidence.{base}.neutral"),
            };
            let owner = side.map(side_name).unwrap_or("");
            Some(fill(
                lookup(&[template_key.as_str()]),
                &[("side", owner), ("items", &join_and(&squares))],
            ))
        }

        "queenside_majority" | "kingside_majority" => {
            let side = side_from_value(value)?;
            Some(fill(
                lookup(&[&format!("evidence.{base}")]),
                &[("side", side_name(side))],
            ))
        }

        "material_diff_cp" => {
            if context.has_pattern {
                return None; // "up the exchange" says it better
            }
            let diff = value.as_i64()?;
            let side = if diff > 0 {
                SideColor::White
            } else {
                SideColor::Black
            };
            let pawns = (diff.abs() + 50) / 100;
            let template_key = match pawns {
                0 => return None, // level material is headline territory
                1 => "evidence.material.pawn_up",
                2 => "evidence.material.two_pawns_up",
                3 => "evidence.material.three_pawns_up",
                _ => "evidence.material.many_pawns_up",
            };
            Some(fill(lookup(&[template_key]), &[("side", side_name(side))]))
        }

        "pattern" => {
            let pattern = value.as_str()?;
            let known = lookup(&[&format!("evidence.pattern.{pattern}")]);
            if known.is_empty() {
                None
            } else {
                Some(known.to_string())
            }
        }

        "open_files" => {
            let files = string_list(value)?;
            let template_key = if files.len() > 1 {
                "evidence.open_files.plural"
            } else {
                "evidence.open_files"
            };
            Some(fill(
                lookup(&[template_key]),
                &[("files", &file_phrase(&files))],
            ))
        }

        "half_open_files" => {
            let files = string_list(value)?;
            let plural = files.len() > 1;
            let template_key = match (key_side.is_some(), plural) {
                (true, true) => "evidence.half_open_files.plural",
                (true, false) => "evidence.half_open_files",
                (false, true) => "evidence.half_open_files.neutral.plural",
                (false, false) => "evidence.half_open_files.neutral",
            };
            let owner = key_side.map(side_name).unwrap_or("");
            Some(fill(
                lookup(&[template_key]),
                &[("files", &file_phrase(&files)), ("side", owner)],
            ))
        }

        "rook_on_seventh" => {
            let side = side_from_value(value)?;
            Some(fill(
                lookup(&["evidence.rook_on_seventh"]),
                &[("side", side_name(side))],
            ))
        }

        "holes" => {
            let squares = string_list(value)?;
            let side = key_side?;
            let template_key = if squares.len() > 1 {
                "evidence.holes.plural"
            } else {
                "evidence.holes"
            };
            Some(fill(
                lookup(&[template_key]),
                &[("side", side_name(side)), ("items", &join_and(&squares))],
            ))
        }

        "established_outpost" => {
            let side = key_side?;
            let square = value.as_str()?;
            Some(fill(
                lookup(&["evidence.established_outpost"]),
                &[("side", side_name(side)), ("items", square)],
            ))
        }

        _ => None,
    }
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
    // A blockade is the DEFENDER's plan: attribute it to the side facing
    // the passer, whatever the parent imbalance favors.
    let favors = match plan.hint.as_str() {
        "BlockadeWhitePasser" => Favors::Black,
        "BlockadeBlackPasser" => Favors::White,
        _ => favors,
    };
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
