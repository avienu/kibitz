//! FeatureRecord v1 — the universal contract between all silman components
//! (docs/SILMAN_ENGINE_SPEC.md). Versioned, serde, JSON-stable: field names
//! and enum spellings below match the spec's JSON sketch exactly and are
//! snapshot-tested; breaking changes bump `SCHEMA_VERSION`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SideColor {
    White,
    Black,
}

impl From<cozy_chess::Color> for SideColor {
    fn from(c: cozy_chess::Color) -> Self {
        match c {
            cozy_chess::Color::White => SideColor::White,
            cozy_chess::Color::Black => SideColor::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Opening,
    Middlegame,
    Endgame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertKind {
    WeakKing,
    TrappedPiece,
    Undefended,
    InadequatelyDefended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineCheckStatus {
    Confirmed,
    Refuted,
    #[serde(rename = "unclear-at-budget")]
    UnclearAtBudget,
}

/// Result of the bounded engine job that verified or refuted an alert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCheck {
    pub status: EngineCheckStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_delta_cp: Option<i32>,
    /// Forced mate distance from the BENEFICIARY's point of view:
    /// positive = the beneficiary mates in N, negative = gets mated,
    /// 0 = the position is already checkmate. When set, `score_delta_cp`
    /// must not be rendered as material (schema v2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mate_in: Option<i32>,
    pub budget_nodes: u64,
}

/// One WSUI tactical alert (docs/SILMAN_ENGINE_SPEC.md Stage 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacticAlert {
    pub kind: AlertKind,
    /// The side that HAS the problem.
    pub side: SideColor,
    /// Primary square ("c6"); absent for diffuse alerts (weak king zone).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attackers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defenders: Vec<String>,
    /// Static exchange value (centipawns, from the attacker's POV).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub see: Option<i32>,
    pub severity: Severity,
    /// Short machine-readable qualifier ("overloaded-defender",
    /// "back-rank", "pawn-shield", "trapped-and-attacked", ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_check: Option<EngineCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WsuiReport {
    pub alerts: Vec<TacticAlert>,
    pub screen_fired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImbalanceKind {
    MinorPieces,
    PawnStructure,
    Material,
    FilesDiagonals,
    SquaresOutposts,
    Space,
    Development,
    Initiative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Favors {
    White,
    Black,
    Balanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Magnitude {
    Minor,
    Clear,
    Winning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanHint {
    pub hint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squares: Vec<String>,
}

/// One positional imbalance (docs/SILMAN_ENGINE_SPEC.md Stage 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Imbalance {
    pub kind: ImbalanceKind,
    pub favors: Favors,
    pub magnitude: Magnitude,
    /// Structured evidence; keys are detector-specific and documented in
    /// the spec ("isolated": ["d5"], "half_open_files": ["d"], ...).
    /// BTreeMap keeps JSON key order deterministic.
    pub evidence: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plans: Vec<PlanHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEval {
    pub eval_cp: i32,
    /// Forced mate distance, White's point of view (schema v2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mate_in: Option<i32>,
    pub best: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multipv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub generator: String,
    pub version: String,
}

/// A composite plan: several PlanHints from independent imbalances that
/// converge on one target (schema v2, run-5 feedback item 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositePlan {
    /// The convergence target ("d5", or a file like "d-file").
    pub target: String,
    /// The clustered hint tokens, strongest first.
    pub hints: Vec<String>,
    /// Distinct imbalance kinds supporting this plan.
    pub supporting: Vec<ImbalanceKind>,
    /// All evidence squares involved (for UI overlays).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squares: Vec<String>,
    /// Ranking score: distinct-support count weighted by magnitude.
    pub score: u32,
    pub favors: Favors,
}

/// One string per narration voice (schema v3). The UI never synthesizes
/// prose: both voices arrive pre-rendered so the voice toggle is instant
/// and provably shows the same facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceText {
    pub coach: String,
    pub neutral: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Alert,
    Imbalance,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowKind {
    Attacker,
    Key,
}

/// A board arrow, always drawn source → target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceArrow {
    pub from: String,
    pub to: String,
    pub kind: ArrowKind,
}

/// The squares/arrows one explanation block asks the board to show, in
/// the shared evidence-overlay language (design/handoff-1). Field names
/// are the UI contract — do not rename.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// Alert-target squares (red ring).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
    /// Attacking pieces' squares (amber wedge + arrow).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attackers: Vec<String>,
    /// Defending pieces' squares (blue wedge, no arrow).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defenders: Vec<String>,
    /// Imbalance squares (green wash).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imbalance: Vec<String>,
    /// Key squares / plan targets (violet wedge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrows: Vec<EvidenceArrow>,
}

/// One sentence block of an explanation: what to say (both voices) and
/// what to show while saying it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplanationBlock {
    pub kind: BlockKind,
    pub text: VoiceText,
    pub evidence: Evidence,
}

/// Numeric readout for the verdict pill / eval bar, White's POV.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReadout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mate: Option<i32>,
    /// Preformatted: "+2.6" or "#5" (negative mate = Black mates).
    pub display: String,
}

/// The per-position explanation object consumed by the game view
/// (schema v3; produced by silman-verbalize::explain — the UI must not
/// synthesize explanations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub schema_version: u32,
    /// Verdict pill text: "TACTICAL SCREEN FIRED" | "FORCED MATE" |
    /// "QUIET POSITION".
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval: Option<EvalReadout>,
    pub headline: VoiceText,
    pub blocks: Vec<ExplanationBlock>,
}

/// The universal contract: everything silman knows about one position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureRecord {
    pub schema_version: u32,
    pub fen: String,
    pub side_to_move: SideColor,
    pub phase: Phase,
    pub wsui: WsuiReport,
    pub imbalances: Vec<Imbalance>,
    /// Synthesized composite plans, best first (schema v2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub composite_plans: Vec<CompositePlan>,
    pub engine: Option<EngineEval>,
    pub provenance: Provenance,
}

impl FeatureRecord {
    pub fn provenance_now() -> Provenance {
        Provenance {
            generator: "silman-core".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Format a cozy-chess square as the record's lowercase string form.
pub fn square_name(sq: cozy_chess::Square) -> String {
    format!("{sq}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_json_shape_matches_spec_sketch() {
        let mut evidence = BTreeMap::new();
        evidence.insert("isolated".to_string(), serde_json::json!(["d5"]));
        evidence.insert("half_open_files".to_string(), serde_json::json!(["d"]));
        let record = FeatureRecord {
            schema_version: SCHEMA_VERSION,
            fen: "rnbqkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 2".into(),
            side_to_move: SideColor::White,
            phase: Phase::Middlegame,
            wsui: WsuiReport {
                alerts: vec![TacticAlert {
                    kind: AlertKind::InadequatelyDefended,
                    side: SideColor::Black,
                    target: Some("c6".into()),
                    attackers: vec!["e5".into(), "d4".into()],
                    defenders: vec!["b7".into()],
                    see: Some(200),
                    severity: Severity::High,
                    detail: None,
                    engine_check: Some(EngineCheck {
                        status: EngineCheckStatus::Confirmed,
                        pv: vec!["Nxc6".into(), "bxc6".into(), "Qd4".into()],
                        score_delta_cp: Some(180),
                        mate_in: None,
                        budget_nodes: 2_000_000,
                    }),
                }],
                screen_fired: true,
            },
            imbalances: vec![Imbalance {
                kind: ImbalanceKind::PawnStructure,
                favors: Favors::White,
                magnitude: Magnitude::Clear,
                evidence,
                plans: vec![PlanHint {
                    hint: "BlockadeThenPressure".into(),
                    squares: vec!["d4".into(), "d5".into()],
                }],
            }],
            composite_plans: vec![],
            engine: None,
            provenance: Provenance {
                generator: "silman-core".into(),
                version: "0.1.0".into(),
            },
        };
        let json = serde_json::to_string_pretty(&record).unwrap();
        // Spec-sketch spellings must hold exactly.
        for needle in [
            "\"schema_version\": 3",
            "\"side_to_move\": \"white\"",
            "\"phase\": \"middlegame\"",
            "\"kind\": \"InadequatelyDefended\"",
            "\"severity\": \"high\"",
            "\"status\": \"confirmed\"",
            "\"screen_fired\": true",
            "\"kind\": \"PawnStructure\"",
            "\"favors\": \"white\"",
            "\"magnitude\": \"clear\"",
            "\"hint\": \"BlockadeThenPressure\"",
        ] {
            assert!(json.contains(needle), "missing {needle} in:\n{json}");
        }
        // Round-trips.
        let back: FeatureRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
        insta::assert_snapshot!(json);
    }
}
