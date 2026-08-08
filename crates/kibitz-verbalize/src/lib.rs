//! kibitz-verbalize: deterministic, template-driven prose from a
//! [`FeatureRecord`] (docs/KIBITZ_ENGINE_SPEC.md, "kibitz-verbalize").
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
mod explain;
#[cfg(feature = "llm")]
pub mod llm;
mod phrase;
mod templates;

use std::cmp::Reverse;
use std::collections::BTreeSet;

use kibitz_core::record::{
    AlertKind, EngineCheck, EngineCheckStatus, Favors, FeatureRecord, Imbalance, Magnitude, Phase,
    PlanHint, SideColor, TacticAlert,
};
use serde_json::Value;

use board::{Board, PieceKind};
pub use explain::{explain, explain_in_book};
use phrase::{
    capitalize_first, decapitalize_first, favors_side, humanize, join_and, magnitude_key,
    pawns_amount, phase_key, see_key, severity_key, side_name,
};
use templates::{fill, lookup, lookup_var, lookup_voiced, rotation, try_lookup_voiced};

/// The three prose sections of a verbalized position, in composition order.
/// Each section is a single paragraph; empty strings mean the record had
/// nothing to say for that section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sections {
    pub tactics: String,
    pub imbalances: String,
    pub plans: String,
    /// The long-horizon paragraph (schema v4). Its own section, not more
    /// plan sentences: a five-move regrouping and "a good plan is X" are
    /// different kinds of advice and reading them as one paragraph is
    /// how the long game gets lost in the noise.
    pub schemes: String,
}

/// Narration voice (run-5 item 3). The voice is a template OVERLAY, not a
/// code fork: Coach reads `coach.<key>` overrides from
/// `templates/coach.tmpl` and falls back to the base key wherever no
/// override exists, so both voices say exactly the same THINGS — only the
/// phrasing differs. Coach is the product default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Voice {
    /// Anthropomorphized Kibitz-style coaching prose (the default): pieces
    /// have desires and grievances, the student is occasionally addressed
    /// directly, and no fact beyond the record is ever added.
    #[default]
    Coach,
    /// The plain base templates, with no anthropomorphizing overlay.
    Neutral,
}

impl Voice {
    /// Both voices, for tests and settings enumeration.
    pub const ALL: [Voice; 2] = [Voice::Coach, Voice::Neutral];

    /// The stable setting token for this voice ("coach" / "neutral").
    pub fn as_str(self) -> &'static str {
        match self {
            Voice::Coach => "coach",
            Voice::Neutral => "neutral",
        }
    }

    /// Lenient parse for stored settings: "neutral" (any case) selects
    /// Neutral; anything else — including absent or corrupt values — falls
    /// back to the Coach default.
    pub fn from_setting(value: &str) -> Voice {
        if value.trim().eq_ignore_ascii_case("neutral") {
            Voice::Neutral
        } else {
            Voice::Coach
        }
    }
}

/// Strict-parse error for [`Voice`] (`FromStr`), used to validate
/// user-supplied setting values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownVoice(pub String);

impl std::fmt::Display for UnknownVoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown voice {:?} (expected coach or neutral)", self.0)
    }
}

impl std::error::Error for UnknownVoice {}

impl std::str::FromStr for Voice {
    type Err = UnknownVoice;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "coach" => Ok(Voice::Coach),
            "neutral" => Ok(Voice::Neutral),
            _ => Err(UnknownVoice(s.to_string())),
        }
    }
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

/// Render the whole record as prose in the default (Coach) voice: tactics,
/// then imbalances, then plans, as blank-line-separated paragraphs. A
/// record with nothing to report yields a single graceful "nothing stands
/// out" line.
/// Above this absolute advantage (centipawns) prose stops counting pawns
/// and states a verdict; the number belongs to the eval readout. Mate
/// rendering (run-5) is untouched and always wins over any band.
pub const DECISIVE_CP: i32 = 500;

/// When an engine-CONFIRMED tactic reaches this swing (or any mate), the
/// position is *about* the tactic: imbalance and plan prose is suppressed
/// entirely. Below it, a confirmed tactic still leads but only the single
/// strongest imbalance is kept (winning a pawn doesn't end the game — the
/// dominant theme stays relevant) and plan talk waits. UNconfirmed static
/// alerts change nothing: the screen may yet be refuted, so hedging with
/// positional context stays honest. (Run-8 maintainer question: "when a
/// tactic is present it seems silly to talk about imbalances".)
pub const TACTIC_DOMINANT_CP: i32 = 200;

/// Strongest engine-confirmed tactical swing in the record's alerts:
/// `i32::MAX` for a confirmed mate, the |cp| delta otherwise, 0 when no
/// alert is confirmed.
pub(crate) fn confirmed_tactic_swing_pub(record: &FeatureRecord) -> i32 {
    confirmed_tactic_swing(record)
}

fn confirmed_tactic_swing(record: &FeatureRecord) -> i32 {
    record
        .wsui
        .alerts
        .iter()
        .filter_map(|a| a.engine_check.as_ref())
        .filter(|c| c.status == EngineCheckStatus::Confirmed)
        .map(|c| {
            if c.mate_in.is_some() {
                i32::MAX
            } else {
                c.score_delta_cp.unwrap_or(0).abs()
            }
        })
        .max()
        .unwrap_or(0)
}

pub fn verbalize(record: &FeatureRecord) -> String {
    verbalize_voiced(record, Voice::default())
}

/// [`verbalize`] with an explicit [`Voice`].
pub fn verbalize_voiced(record: &FeatureRecord, voice: Voice) -> String {
    let sections = verbalize_sections_voiced(record, voice);
    if sections.tactics.is_empty()
        && sections.imbalances.is_empty()
        && sections.plans.is_empty()
        && sections.schemes.is_empty()
    {
        return lookup_voiced(voice, &["empty"]).to_string();
    }
    let mut paragraphs: Vec<String> = Vec::new();
    if sections.tactics.is_empty() {
        paragraphs.push(lookup_voiced(voice, &["tactics.quiet"]).to_string());
    } else {
        paragraphs.push(sections.tactics);
    }
    if !sections.imbalances.is_empty() {
        paragraphs.push(sections.imbalances);
    }
    if !sections.plans.is_empty() {
        paragraphs.push(sections.plans);
    }
    if !sections.schemes.is_empty() {
        paragraphs.push(sections.schemes);
    }
    paragraphs.join("\n\n")
}

/// Render the record into its three sections without joining them, in the
/// default (Coach) voice, for callers (the app UI) that lay the sections
/// out separately.
pub fn verbalize_sections(record: &FeatureRecord) -> Sections {
    verbalize_sections_voiced(record, Voice::default())
}

/// [`verbalize_sections`] with an explicit [`Voice`].
pub fn verbalize_sections_voiced(record: &FeatureRecord, voice: Voice) -> Sections {
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
            let sentences: Vec<String> = render_alert_sentences(alert, &board, voice)
                .into_iter()
                .filter(|sentence| seen_clauses.insert(sentence.clone()))
                .collect();
            apply_starter(voice, "rotation.alert", index, sentences.join(" "))
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    // Tactics-first gate: a confirmed tactic mutes positional prose in
    // proportion to its size (see TACTIC_DOMINANT_CP).
    let swing = confirmed_tactic_swing(record);
    let selected = if swing >= TACTIC_DOMINANT_CP {
        Vec::new()
    } else if swing > 0 {
        select_imbalances(&record.imbalances)
            .into_iter()
            .take(1)
            .collect()
    } else {
        select_imbalances(&record.imbalances)
    };

    let mut imbalance_sentences: Vec<String> = selected
        .iter()
        .enumerate()
        .map(|(index, imbalance)| {
            apply_starter(
                voice,
                "rotation.imbalance",
                index,
                render_imbalance(imbalance, &board, record.phase, voice),
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
                lookup_voiced(voice, &["engine.eval.mate"]),
                &[
                    ("mate", &mate.abs().to_string()),
                    ("side", side),
                    ("best", &engine.best),
                ],
            ));
        } else if engine.eval_cp.abs() >= DECISIVE_CP {
            let side = side_name(if engine.eval_cp > 0 {
                SideColor::White
            } else {
                SideColor::Black
            });
            imbalance_sentences.push(fill(
                lookup_voiced(voice, &["engine.eval.decisive"]),
                &[("side", side), ("best", &engine.best)],
            ));
        } else {
            let score = format!("{:+.1}", f64::from(engine.eval_cp) / 100.0);
            imbalance_sentences.push(fill(
                lookup_voiced(voice, &["engine.eval"]),
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
    let composites: &[_] = if swing > 0 {
        &[]
    } else {
        &record.composite_plans[..]
    };
    let covered = scheme_covered(record);
    for (i, cp) in composites.iter().take(2).enumerate() {
        if cp.supporting.len() < 2 {
            continue;
        }
        for h in &cp.hints {
            seen.insert(h.as_str());
        }
        if covered.contains(&cp.target) {
            continue; // the scheme paragraph says this, in order
        }
        plan_sentences.push(render_composite(cp, i, voice));
    }
    for imbalance in &selected {
        if swing > 0 {
            break; // tactics on the board: plan talk waits for the verdict
        }
        for plan in &imbalance.plans {
            if eclipsed_by_sibling(&plan.hint, &imbalance.plans) {
                continue;
            }
            if plan.squares.last().is_some_and(|sq| covered.contains(sq)) {
                continue;
            }
            if seen.insert(plan.hint.as_str()) {
                plan_sentences.push(render_plan(
                    plan,
                    imbalance.favors,
                    plan_sentences.len(),
                    voice,
                ));
            }
        }
    }

    let scheme_source: &[_] = if swing > 0 { &[] } else { &record.schemes[..] };
    let mut scheme_sentences: Vec<String> = scheme_source
        .iter()
        .take(2)
        .enumerate()
        .map(|(i, sc)| render_scheme(sc, &record.fen, i, voice))
        .filter(|s| !s.is_empty())
        .collect();
    // Maneuvers no scheme absorbed still get said.
    if swing == 0 {
        for m in record.maneuvers.iter().take(2) {
            if record.schemes.iter().any(|sc| sc.target == m.to) {
                continue;
            }
            scheme_sentences.push(render_maneuver(m, voice));
        }
    }

    Sections {
        tactics,
        imbalances,
        schemes: scheme_sentences.join(" "),
        plans: plan_sentences.join(" "),
    }
}

/// Prefix the paragraph with its rotation starter. Without a starter the
/// first letter is capitalized; with one, the sentence is decapitalized
/// unless it opens with a side name (a proper noun).
fn apply_starter(voice: Voice, rotation_key: &str, index: usize, paragraph: String) -> String {
    let starter = rotation(voice, rotation_key, index);
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

pub(crate) fn render_alert_sentences(
    alert: &TacticAlert,
    board: &Board,
    voice: Voice,
) -> Vec<String> {
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
            lookup_voiced(voice, &["alert.overloaded"]),
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
        .is_some_and(|key| !lookup_voiced(voice, &[key]).is_empty());
    let severity_lead_key = format!("alert.{kind}.{}", severity_key(alert.severity));
    let mut lead_keys: Vec<&str> = Vec::new();
    if let Some(key) = detail_lead_key.as_deref() {
        lead_keys.push(key);
    }
    lead_keys.push(severity_lead_key.as_str());
    lead_keys.push("alert.generic");
    let mut sentences = vec![fill(
        lookup_var(voice, &lead_keys, seed),
        &[("subject", &subject), ("target_clause", &target_clause)],
    )];

    if alert.kind == AlertKind::WeakKing {
        if !attackers.is_empty() {
            sentences.push(fill(
                lookup_voiced(voice, &["clause.king_attackers"]),
                &[("attackers", &attackers)],
            ));
        }
    } else if alert.kind == AlertKind::Undefended {
        // The lead sentence already states that nothing defends it.
        if !attackers.is_empty() {
            sentences.push(fill(
                lookup_voiced(voice, &["clause.attacked_by"]),
                &[("attackers", &attackers)],
            ));
        }
    } else {
        match (attackers.is_empty(), defenders.is_empty()) {
            (false, false) => sentences.push(fill(
                lookup_var(voice, &["clause.attack_defend.both"], seed),
                &[("attackers", &attackers), ("defenders", &defenders)],
            )),
            (false, true) => sentences.push(fill(
                lookup_voiced(voice, &["clause.attack_defend.attackers"]),
                &[("attackers", &attackers)],
            )),
            (true, false) => {
                let key = if alert.defenders.len() > 1 {
                    "clause.attack_defend.defenders.plural"
                } else {
                    "clause.attack_defend.defenders"
                };
                sentences.push(fill(
                    lookup_voiced(voice, &[key]),
                    &[("defenders", &defenders)],
                ));
            }
            (true, true) => {}
        }
    }

    if let Some(token) = detail {
        if !detail_in_lead {
            if alert.kind == AlertKind::WeakKing {
                sentences.push(render_king_details(token, voice));
            } else {
                let known = lookup_voiced(voice, &[&format!("detail.{token}")]);
                if known.is_empty() {
                    sentences.push(fill(
                        lookup_voiced(voice, &["clause.detail"]),
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
                lookup_var(voice, &["clause.see"], seed),
                &[("amount", lookup(&[see_key(see)]))],
            ));
        }
    }

    if let Some(check) = &alert.engine_check {
        sentences.push(render_engine_check(check, voice));
    }

    sentences
}

/// WeakKing `detail` is a semicolon-joined compound ("zone-pressure+3;
/// g-file shield pawn missing; open-files:g; back-rank"). Each component is
/// rewritten as a natural clause and the clauses joined into one sentence;
/// the zone-pressure count is folded away, and a shield file already
/// reported as wide open is not also reported as missing its pawn.
fn render_king_details(detail: &str, voice: Voice) -> String {
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
        clauses.push(lookup_voiced(voice, &["king.zone_pressure"]).to_string());
    }
    if !missing.is_empty() {
        clauses.push(fill(
            lookup_voiced(voice, &["king.shield_missing"]),
            &[("files", &file_phrase(&missing))],
        ));
    }
    if !advanced.is_empty() {
        let key = if advanced.len() > 1 {
            "king.shield_advanced.plural"
        } else {
            "king.shield_advanced"
        };
        clauses.push(fill(
            lookup_voiced(voice, &[key]),
            &[("files", &file_phrase(&advanced))],
        ));
    }
    if !open.is_empty() {
        let key = if open.len() > 1 {
            "king.open_files.plural"
        } else {
            "king.open_files"
        };
        clauses.push(fill(
            lookup_voiced(voice, &[key]),
            &[("files", &file_phrase(&open))],
        ));
    }
    if back_rank {
        clauses.push(lookup_voiced(voice, &["king.back_rank"]).to_string());
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

fn render_engine_check(check: &EngineCheck, voice: Voice) -> String {
    let engine_key = |key: &str| lookup_voiced(voice, &[key]);
    match check.status {
        EngineCheckStatus::Confirmed => {
            let pv = check.pv.join(" ");
            // Mate distances take absolute priority over material units
            // (run-5 bug 1): a mate score must never read as pawns.
            if let Some(mate) = check.mate_in {
                let m = mate.abs().to_string();
                return if mate == 0 {
                    fill(engine_key("engine.confirmed.pv_mate_now"), &[("pv", &pv)])
                } else if mate < 0 {
                    fill(engine_key("engine.confirmed.mate_against"), &[("mate", &m)])
                } else if pv.is_empty() {
                    fill(engine_key("engine.confirmed.mate"), &[("mate", &m)])
                } else {
                    fill(
                        engine_key("engine.confirmed.pv_mate"),
                        &[("pv", &pv), ("mate", &m)],
                    )
                };
            }
            // Large finite advantages read as a verdict, not a pawn
            // count (run-6 residual): the number lives in the eval
            // readout, not the prose.
            if check.score_delta_cp.is_some_and(|d| d.abs() >= DECISIVE_CP) {
                return if pv.is_empty() {
                    engine_key("engine.confirmed.decisive").to_string()
                } else {
                    fill(engine_key("engine.confirmed.pv_decisive"), &[("pv", &pv)])
                };
            }
            match (check.pv.is_empty(), check.score_delta_cp) {
                (false, Some(delta)) => fill(
                    engine_key("engine.confirmed.pv_delta"),
                    &[("pv", &pv), ("delta", &pawns_amount(delta))],
                ),
                (false, None) => fill(engine_key("engine.confirmed.pv"), &[("pv", &pv)]),
                (true, Some(delta)) => fill(
                    engine_key("engine.confirmed.delta"),
                    &[("delta", &pawns_amount(delta))],
                ),
                (true, None) => engine_key("engine.confirmed").to_string(),
            }
        }
        EngineCheckStatus::Refuted => engine_key("engine.refuted").to_string(),
        EngineCheckStatus::UnclearAtBudget => engine_key("engine.unclear").to_string(),
    }
}

pub(crate) fn render_imbalance(
    imbalance: &Imbalance,
    board: &Board,
    phase: Phase,
    voice: Voice,
) -> String {
    let kind = format!("{:?}", imbalance.kind);

    // Cross-key context: "open" flavors the bishop-pair phrase, and a named
    // material pattern supersedes the raw centipawn figure.
    let context = EvidenceContext {
        open_position: imbalance.evidence.get("character").and_then(Value::as_str) == Some("open"),
        has_pattern: imbalance.evidence.contains_key("pattern"),
        piece_diff: imbalance.evidence.get("piece_diff").cloned(),
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
        .filter_map(|(key, value)| render_evidence(key, value, board, &context, voice))
        .map(|clause| format!("{}.", capitalize_first(&clause)))
        .collect();

    let aspect_key = format!("aspect.{kind}");
    let known_aspect = lookup_voiced(voice, &[aspect_key.as_str()]);
    let aspect = if known_aspect.is_empty() {
        humanize(&kind)
    } else {
        known_aspect.to_string()
    };

    // The development PRIOR (run 11) is a to-do list, not an advantage:
    // its headline must never claim {beneficiary} is ahead. Prior
    // imbalances are recognized by their evidence keys, so position-only
    // records (the plain development detector) keep the classic phrasing.
    let development_prior = kind == "Development"
        && imbalance.evidence.keys().any(|key| {
            matches!(
                evidence_base_key(key),
                "sleeping_minors"
                    | "king_in_center"
                    | "queen_sortie"
                    | "wanderer"
                    | "center_unclaimed"
            )
        });

    let headline = match favors_side(imbalance.favors) {
        None => {
            let balanced_key = format!("imbalance.{kind}.balanced");
            fill(
                lookup_voiced(voice, &[balanced_key.as_str(), "imbalance.balanced"]),
                &[("aspect", &aspect)],
            )
        }
        Some(side) => {
            let magnitude = magnitude_key(imbalance.magnitude);
            let todo_key = format!("imbalance.{kind}.todo.{magnitude}");
            let phased_key = format!("imbalance.{kind}.{magnitude}.{}", phase_key(phase));
            let plain_key = format!("imbalance.{kind}.{magnitude}");
            let generic_key = format!("imbalance.generic.{magnitude}");
            let mut keys: Vec<&str> = Vec::new();
            if development_prior {
                keys.push(todo_key.as_str());
            }
            keys.extend([
                phased_key.as_str(),
                plain_key.as_str(),
                generic_key.as_str(),
            ]);
            fill(
                lookup_voiced(voice, &keys),
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
    /// Per-piece surplus (white minus black), when the detector supplied
    /// it — lets material speak in piece terms.
    piece_diff: Option<Value>,
}

/// Chess-terms phrase for a piece surplus, or None when plain pawn
/// counting says it best. `side` is the side the cp diff favors.
fn material_phrase(pd: &Value, side: SideColor, voice: Voice) -> Option<String> {
    let get = |k: &str| pd.get(k).and_then(Value::as_i64).unwrap_or(0);
    // Orient so positive = the favored side's surplus.
    let sign = if side == SideColor::White { 1 } else { -1 };
    let (p, n, b, r, q) = (
        get("p") * sign,
        get("n") * sign,
        get("b") * sign,
        get("r") * sign,
        get("q") * sign,
    );
    let minors = n + b;
    let key: &str = if q > 0 && minors <= 0 && r >= 0 {
        "evidence.material.queen_up"
    } else if r > 0 && minors == 0 && p >= 0 && q >= 0 {
        "evidence.material.rook_up"
    } else if minors > 0 && p == 0 && r >= 0 && q >= 0 {
        "evidence.material.piece_up"
    } else if minors > 0 && p == -1 {
        "evidence.material.piece_for_pawn"
    } else if minors > 0 && p <= -2 {
        "evidence.material.piece_for_pawns"
    } else {
        return None; // pure pawn surpluses etc. read fine as counts
    };
    Some(fill(
        lookup_voiced(voice, &[key]),
        &[("side", side_name(side))],
    ))
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
    voice: Voice,
) -> Option<String> {
    // Doubled majors carry the file in the key ("doubled_majors_d") and the
    // owning side in the value.
    if let Some(file) = key.strip_prefix("doubled_majors_") {
        let side = side_from_value(value)?;
        return Some(fill(
            lookup_voiced(voice, &["evidence.doubled_majors"]),
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
            Some("closed") => {
                Some(lookup_voiced(voice, &["evidence.character.closed"]).to_string())
            }
            _ => None,
        },

        "bishop_pair" => {
            let side = side_from_value(value).or(key_side)?;
            let template_key = if context.open_position {
                "evidence.bishop_pair.open"
            } else {
                "evidence.bishop_pair"
            };
            Some(fill(
                lookup_voiced(voice, &[template_key]),
                &[("side", side_name(side))],
            ))
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
                    lookup_voiced(voice, &["evidence.bad_bishop.bare"]),
                    &[("side", side_name(side)), ("square", square)],
                ))
            } else {
                Some(fill(
                    lookup_voiced(voice, &["evidence.bad_bishop"]),
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
                lookup_voiced(voice, &[template_key.as_str()]),
                &[("side", owner), ("items", &join_and(&squares))],
            ))
        }

        "queenside_majority" | "kingside_majority" => {
            let side = side_from_value(value)?;
            Some(fill(
                lookup_voiced(voice, &[&format!("evidence.{base}")]),
                &[("side", side_name(side))],
            ))
        }

        // Raw per-piece surplus: consumed by material_diff_cp below, never
        // narrated on its own.
        "piece_diff" => None,

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
            if pawns == 0 {
                return None; // level material is headline territory
            }
            // Name the surplus in chess terms when the piece mix says
            // more than a pawn count ("up a piece", "a piece for two
            // pawns") — run-9 maintainer report: a won knight is not
            // "three pawns".
            if let Some(named) = context
                .piece_diff
                .as_ref()
                .and_then(|pd| material_phrase(pd, side, voice))
            {
                return Some(named);
            }
            let template_key = match pawns {
                1 => "evidence.material.pawn_up",
                2 => "evidence.material.two_pawns_up",
                3 => "evidence.material.three_pawns_up",
                _ => "evidence.material.many_pawns_up",
            };
            Some(fill(
                lookup_voiced(voice, &[template_key]),
                &[("side", side_name(side))],
            ))
        }

        "pattern" => {
            let pattern = value.as_str()?;
            let known = lookup_voiced(voice, &[&format!("evidence.pattern.{pattern}")]);
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
                lookup_voiced(voice, &[template_key]),
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
                lookup_voiced(voice, &[template_key]),
                &[("files", &file_phrase(&files)), ("side", owner)],
            ))
        }

        "rook_on_seventh" => {
            let side = side_from_value(value)?;
            Some(fill(
                lookup_voiced(voice, &["evidence.rook_on_seventh"]),
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
                lookup_voiced(voice, &[template_key]),
                &[("side", side_name(side)), ("items", &join_and(&squares))],
            ))
        }

        "established_outpost" => {
            let side = key_side?;
            let square = value.as_str()?;
            Some(fill(
                lookup_voiced(voice, &["evidence.established_outpost"]),
                &[("side", side_name(side)), ("items", square)],
            ))
        }

        // --- Development prior (run 11) ---
        "sleeping_minors" => {
            let squares = string_list(value)?;
            let side = key_side?;
            let kinds: Vec<Option<PieceKind>> = squares
                .iter()
                .map(|square| board.piece_at(square).map(|(_, kind)| kind))
                .collect();
            let knights = kinds
                .iter()
                .filter(|k| **k == Some(PieceKind::Knight))
                .count();
            let bishops = kinds
                .iter()
                .filter(|k| **k == Some(PieceKind::Bishop))
                .count();
            let template_key = if squares.len() == 1 {
                match kinds[0] {
                    Some(PieceKind::Knight) => "evidence.sleeping_minors.one_knight",
                    Some(PieceKind::Bishop) => "evidence.sleeping_minors.one_bishop",
                    _ => "evidence.sleeping_minors.one",
                }
            } else if knights > 0 && bishops > 0 {
                "evidence.sleeping_minors.mixed"
            } else if knights > 0 {
                "evidence.sleeping_minors.knights"
            } else if bishops > 0 {
                "evidence.sleeping_minors.bishops"
            } else {
                "evidence.sleeping_minors.plural"
            };
            Some(fill(
                lookup_voiced(voice, &[template_key, "evidence.sleeping_minors.plural"]),
                &[("side", side_name(side)), ("items", &join_and(&squares))],
            ))
        }

        "king_in_center" => {
            let side = key_side?;
            let state_key = match value.as_str()? {
                "lost" => "evidence.king_in_center.lost",
                _ => "evidence.king_in_center.available",
            };
            Some(fill(
                lookup_voiced(voice, &[state_key]),
                &[("side", side_name(side))],
            ))
        }

        "queen_sortie" => {
            let side = key_side?;
            let square = value.as_str()?;
            Some(fill(
                lookup_voiced(voice, &["evidence.queen_sortie"]),
                &[("side", side_name(side)), ("items", square)],
            ))
        }

        "wanderer" => {
            let side = key_side?;
            let object = value.as_object()?;
            let square = object.get("square")?.as_str()?;
            let times_key = match object.get("times").and_then(Value::as_u64) {
                Some(2) => "count.twice",
                Some(3) => "count.three_times",
                _ => "count.many_times",
            };
            Some(fill(
                lookup_voiced(voice, &["evidence.wanderer"]),
                &[
                    ("side", side_name(side)),
                    ("square", square),
                    ("times", lookup(&[times_key])),
                ],
            ))
        }

        "center_unclaimed" => {
            let squares = string_list(value)?;
            let side = key_side?;
            let template_key = if squares.len() > 1 {
                "evidence.center_unclaimed.plural"
            } else {
                "evidence.center_unclaimed"
            };
            Some(fill(
                lookup_voiced(voice, &[template_key]),
                &[("side", side_name(side)), ("items", &join_and(&squares))],
            ))
        }

        _ => None,
    }
}

/// The single quiet line shown while a position is still in the openings
/// book (run 11). The BOOK STATE itself is the caller's knowledge —
/// kibitz-core/-verbalize never touch a database; the caller passes
/// `in_book` and this renders the line in the requested voice.
pub fn book_line(voice: Voice) -> String {
    lookup_voiced(voice, &["book.in_theory"]).to_string()
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

pub(crate) fn render_plan(plan: &PlanHint, favors: Favors, index: usize, voice: Voice) -> String {
    // Whose plan it is, from the hint. This was the third copy of a
    // name-based guess at attribution, and it is the one that MATTERS:
    // the other two decide ranking, this one decides which player the
    // sentence names. Retiring the sided-plan filter means plans owned by
    // the side the imbalance disfavours now reach here, so reading the
    // owner is what makes that safe rather than a regression.
    let favors = plan.attributed(favors);
    let hint_key = format!("plan.{}", plan.hint);
    let known = lookup_voiced(voice, &[hint_key.as_str()]);
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
                lookup_voiced(voice, &["clause.plan_squares"]),
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
        lookup_voiced(voice, &[lead_key]),
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

/// Dominance selection shared by narration and the explanation builder:
/// winning > clear > minor; with three or more imbalances the minor ones
/// are noise unless nothing stronger exists.
pub(crate) fn select_imbalances(imbalances: &[Imbalance]) -> Vec<&Imbalance> {
    let mut ranked: Vec<&Imbalance> = imbalances.iter().collect();
    ranked.sort_by_key(|imbalance| Reverse(imbalance.magnitude));
    let drop_minor = imbalances.len() >= 3 && ranked.iter().any(|i| i.magnitude > Magnitude::Minor);
    if drop_minor {
        ranked
            .into_iter()
            .filter(|i| i.magnitude > Magnitude::Minor)
            .collect()
    } else {
        ranked
    }
}

/// The "what to play" closing sentence for a narrated position (run 10),
/// or `None` when there is nothing sound to suggest or a mate/decisive
/// engine line owns the position (tactics outrank plans — callers already
/// suppress plan prose, and therefore this closing, while a confirmed
/// tactic stands). Callers decide WHEN a closing is appropriate — the
/// narrator, for instance, skips capture plies, where the only honest
/// advice is to finish the exchange.
pub fn suggestion_closing(record: &FeatureRecord, voice: Voice) -> Option<String> {
    suggestion_closing_verified(record, voice, None)
}

/// [`suggestion_closing`] with engine-verification context (run 11).
///
/// `cleared` carries the uci moves a bounded engine review cleared for
/// this position (the wsui-confirm job's cursory candidate searches):
/// only those render — refuted candidates vanish even when statically
/// clean. `None` means no engine ran; then the static whole-board veto
/// governs and marked candidates (`static_risk`) are dropped — bad
/// advice is worse than no advice, even though this conservatively
/// drops engine-clearable moves too.
pub fn suggestion_closing_verified(
    record: &FeatureRecord,
    voice: Voice,
    cleared: Option<&[String]>,
) -> Option<String> {
    let decisive = record
        .engine
        .as_ref()
        .is_some_and(|engine| engine.mate_in.is_some() || engine.eval_cp.abs() >= DECISIVE_CP);
    if decisive {
        return None;
    }
    let board = record.fen.parse::<kibitz_core::cozy_chess::Board>().ok()?;
    let mut suggestions = kibitz_core::suggest::suggest(record, &board);
    match cleared {
        Some(list) => suggestions.retain(|s| list.contains(&s.mv)),
        None => suggestions.retain(|s| s.static_risk.is_none()),
    }
    let closing = render_suggestions(&suggestions, voice);
    (!closing.is_empty()).then_some(closing)
}

/// The rendered sentence for a suggestion list: empty when the list is.
/// A prophylactic top pick names the opponent plan being denied (base
/// verb phrase, truncated at any semicolon so it reads inside the
/// sentence); otherwise the sentence states how many plans the top move
/// serves — counts are spelled out, never digits.
pub(crate) fn render_suggestions(
    suggestions: &[kibitz_core::suggest::Suggestion],
    voice: Voice,
) -> String {
    let Some(top) = suggestions.first() else {
        return String::new();
    };
    if top.prophylactic {
        if let Some(token) = top.serving.first() {
            let key = format!("plan.{token}");
            let known = lookup(&[key.as_str()]);
            let phrase = if known.is_empty() {
                humanize(token)
            } else {
                known.split(';').next().unwrap_or(known).trim().to_string()
            };
            return fill(
                lookup_voiced(voice, &["suggest.prophylactic"]),
                &[("first", &top.san), ("their_plan", &phrase)],
            );
        }
    }
    let second = suggestions.get(1).map(|s| s.san.as_str());
    let many = top.serving.len() >= 2;
    let key = match (many, second.is_some()) {
        (true, true) => "suggest.close",
        (true, false) => "suggest.close.solo",
        (false, true) => "suggest.close.one",
        (false, false) => "suggest.close.one.solo",
    };
    let count_key = match top.serving.len() {
        2 => "suggest.count.two",
        3 => "suggest.count.three",
        4 => "suggest.count.four",
        _ => "suggest.count.many",
    };
    fill(
        lookup_voiced(voice, &[key]),
        &[
            ("first", &top.san),
            ("second", second.unwrap_or("")),
            ("n", lookup(&[count_key])),
        ],
    )
}

/// Render one composite plan: index 0 is the unified lead, later indices
/// the brief runner-up.
/// Squares a scheme already narrates in full, for the side that owns it.
///
/// A scheme states the whole campaign in order — clear the guard, come
/// in, cash in. Repeating its parts as loose plan sentences ("reroute
/// the knight there", "trade off the piece guarding it", "walk the
/// bishop round") tells the reader the same thing four times in two
/// paragraphs. The long-term paragraph wins; the plan-level chatter
/// about that square goes.
pub(crate) fn scheme_covered(record: &FeatureRecord) -> std::collections::BTreeSet<String> {
    record.schemes.iter().map(|s| s.target.clone()).collect()
}

/// True when a hint says the same thing as a sibling already being
/// narrated. The RECORD keeps both — they score separately and mean
/// subtly different things (own the majority vs. cash it into a passer) —
/// but prose that makes the same point three sentences running reads as
/// padding, so the quieter one stays quiet.
pub(crate) fn eclipsed_by_sibling(hint: &str, siblings: &[kibitz_core::record::PlanHint]) -> bool {
    match hint {
        // The general weak-pawn statement yields to the specific one when
        // both name the same pawn: "pile up on the backward d6 pawn" says
        // strictly more than "pile up on a pawn nobody can defend".
        "TargetWeakPawn" => siblings
            .iter()
            .any(|s| s.hint == "PressureBackwardPawn" || s.hint == "PressureDoubledPawn"),
        "CreatePassedPawn" => siblings
            .iter()
            .any(|s| s.hint == "AdvanceQueensideMajority" || s.hint == "AdvanceCentralMajority"),
        _ => false,
    }
}

/// Render a standalone [`Maneuver`] — one no scheme picked up. Without
/// this a record can hold a plan the reader never hears about, which is
/// how the opposition (a Maneuver by design, since it belongs to the side
/// to move rather than the side who stands better) would go unsaid.
pub(crate) fn render_maneuver(m: &kibitz_core::record::Maneuver, voice: Voice) -> String {
    let route: Vec<String> = std::iter::once(m.from.clone())
        .chain(m.via.iter().cloned())
        .chain([m.to.clone()])
        .collect();
    let clause = fill(
        lookup_voiced(
            voice,
            &[&format!("maneuver.{}", m.reason), "maneuver.generic"],
        ),
        &[
            ("piece", &m.piece),
            ("from", &m.from),
            ("to", &m.to),
            ("route", &route.join("-")),
        ],
    );
    fill(
        lookup_voiced(voice, &["maneuver.lead"]),
        &[("side", side_name_favors(m.favors)), ("clause", &clause)],
    )
}

/// Render a [`Scheme`] as one ordered sentence: the prerequisite, the
/// way in (with alternatives), and the payoff, in that order.
///
/// This is the long-horizon voice. Everything else the verbalizer says is
/// about the position as it stands; a scheme is the only thing that says
/// "and then", which is exactly what a plan is.
pub(crate) fn render_scheme(
    scheme: &kibitz_core::record::Scheme,
    fen: &str,
    index: usize,
    voice: Voice,
) -> String {
    let board = crate::board::Board::from_fen(fen);
    let piece_on = |sq: &str| -> String {
        board
            .piece_at(sq)
            .map(|(_, kind)| lookup(&[kind.template_key()]).to_string())
            .unwrap_or_else(|| lookup(&["piece.generic"]).to_string())
    };

    let mut clauses: Vec<String> = Vec::new();
    let mut seen_maneuver = false;
    for step in &scheme.steps {
        let via = step.via.join("-");
        match step.kind.as_str() {
            "clear" => {
                // Name the piece, not the square: "the knight on f6" is a
                // thing you can go and trade; "the f6" is not English.
                let named: Vec<String> = step
                    .squares
                    .iter()
                    .map(|sq| {
                        fill(
                            lookup_voiced(voice, &["scheme.target_piece"]),
                            &[("piece", &piece_on(sq)), ("square", sq)],
                        )
                    })
                    .collect();
                let targets = join_and(&named);
                let text = match &step.agent {
                    Some(agent) => {
                        let via_clause = if step.via.is_empty() {
                            String::new()
                        } else {
                            fill(
                                lookup_voiced(voice, &["scheme.step.clear.via_clause"]),
                                &[("via", &via)],
                            )
                        };
                        fill(
                            lookup_voiced(voice, &["scheme.step.clear.by"]),
                            &[
                                ("targets", &targets),
                                ("piece", &piece_on(agent)),
                                ("agent", agent),
                                ("via_clause", &via_clause),
                            ],
                        )
                    }
                    // No agent means we cannot get at the defender. Say so
                    // rather than issuing an instruction nobody can follow.
                    None => fill(
                        lookup_voiced(voice, &["scheme.step.clear.nobody"]),
                        &[("targets", &targets)],
                    ),
                };
                clauses.push(text);
            }
            "maneuver" => {
                let route = step.squares.join("-");
                let from = step.squares.first().cloned().unwrap_or_default();
                let to = step.squares.last().cloned().unwrap_or_default();
                let key = if seen_maneuver {
                    "scheme.step.maneuver.alt"
                } else {
                    "scheme.step.maneuver"
                };
                seen_maneuver = true;
                clauses.push(fill(
                    lookup_voiced(voice, &[key]),
                    &[
                        ("piece", &piece_on(&from)),
                        ("agent", &from),
                        ("to", &to),
                        ("route", &route),
                    ],
                ));
            }
            "exploit" => {
                let Some(hint) = step.hint.as_deref() else {
                    continue;
                };
                let plan = try_lookup_voiced(voice, &format!("plan.composite.clause.{hint}"))
                    .map(str::to_string)
                    .unwrap_or_else(|| humanize(hint));
                clauses.push(fill(
                    lookup_voiced(voice, &["scheme.step.exploit"]),
                    &[("plan", &plan)],
                ));
            }
            _ => {}
        }
    }
    if clauses.is_empty() {
        return String::new();
    }

    // Template values are trimmed by the store, so the separating space
    // belongs here rather than in the data file.
    let join = format!("{} ", lookup_voiced(voice, &["scheme.join"]));
    let steps_text = clauses.join(&join);
    let horizon_clause = if scheme.horizon > 0 {
        format!(
            " {}",
            fill(
                lookup_voiced(voice, &["scheme.horizon_clause"]),
                &[("horizon", &moves_phrase(scheme.horizon))],
            )
        )
    } else {
        String::new()
    };
    let key = if index == 0 {
        "scheme.lead"
    } else {
        "scheme.lead.more"
    };
    fill(
        lookup_voiced(voice, &[key]),
        &[
            ("side", side_name_favors(scheme.favors)),
            ("target", &scheme.target),
            ("horizon_clause", &horizon_clause),
            ("steps", &steps_text),
        ],
    )
}

/// "one move" / "four moves" — the horizon is an estimate, so it reads as
/// words rather than pretending to be a measurement.
fn moves_phrase(moves: u8) -> String {
    let word = match moves {
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        _ => return format!("{moves} moves"),
    };
    if moves == 1 {
        format!("{word} move")
    } else {
        format!("{word} moves")
    }
}

pub(crate) fn render_composite(
    cp: &kibitz_core::record::CompositePlan,
    index: usize,
    voice: Voice,
) -> String {
    if index == 0 {
        let mut clauses: Vec<String> = cp
            .hints
            .iter()
            .filter_map(|h| {
                try_lookup_voiced(voice, &format!("plan.composite.clause.{h}")).map(str::to_string)
            })
            .collect();
        // The same hint can enter a cluster twice (two squares, one
        // idea) — each clause reads once (run-9 maintainer screenshot).
        let mut seen_clause = std::collections::HashSet::new();
        clauses.retain(|c| seen_clause.insert(c.clone()));
        let clause_text = if clauses.is_empty() {
            humanize(&cp.hints.join(", "))
        } else {
            join_and(&clauses)
        };
        fill(
            lookup_voiced(voice, &["plan.composite.lead"]),
            &[("target", &cp.target), ("clauses", &clause_text)],
        )
    } else {
        fill(
            lookup_voiced(voice, &["plan.composite.runner_up"]),
            &[
                ("target", &cp.target),
                ("side", side_name_favors(cp.favors)),
            ],
        )
    }
}
