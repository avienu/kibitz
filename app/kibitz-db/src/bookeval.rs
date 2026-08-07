//! Book-trial evaluation (run 8.5): score the static analyzer against a
//! PRIVATE corpus of positions transcribed from the maintainer's own
//! chess books (testdata/private/book-trials/*.json, git-ignored).
//!
//! Each corpus entry carries a FEN plus expectations expressed in OUR
//! vocabulary (ImbalanceKind names, PlanHint tokens, AlertKind names) —
//! never book prose. The harness reports recall per axis and surfaces
//! "vocabulary gaps": free-form expectation tags our engine has no hint
//! for yet. No engine, no network — pure static analysis.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Corpus {
    pub book: String,
    #[serde(default)]
    pub edition: String,
    pub positions: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub citation: String,
    pub fen: String,
    /// Optional SAN move list from the standard start reaching `fen`
    /// (run 11): principle entries reconstruct exactly, and the
    /// development tracker needs the history for its wandering/tempo
    /// observations. A list that fails to replay to `fen` is ignored
    /// (the tracker then sees the bare position).
    #[serde(default)]
    pub sans: Vec<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub expected: Expected,
    #[serde(default)]
    pub not_expected: NotExpected,
}

/// Negative assertions: things the analyzer must NOT claim here. The
/// counter-example positions (a "useful" doubled pawn, an open file with
/// no entry squares...) anchor precision while thresholds chase recall.
#[derive(Debug, Default, Deserialize)]
pub struct NotExpected {
    #[serde(default)]
    pub imbalances: Vec<String>,
    #[serde(default)]
    pub plan_tags: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Expected {
    #[serde(default)]
    pub favors: Option<String>,
    #[serde(default)]
    pub imbalances: Vec<String>,
    #[serde(default)]
    pub plan_tags: Vec<String>,
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default)]
    pub theme: String,
    /// Book-given best move(s) in SAN, when the transcription carried
    /// them — scored against kibitz-core::suggest (run 10).
    #[serde(default)]
    pub best_moves: Vec<String>,
}

/// Known PlanHint tokens the engine can emit today. Expected plan tags
/// outside this list are vocabulary gaps, reported rather than scored.
/// Plan token a `Maneuver::reason` stands for, where it has one.
fn maneuver_token(reason: &str) -> Option<&'static str> {
    match reason {
        "opposition" => Some("TakeOpposition"),
        _ => None,
    }
}

const KNOWN_HINTS: &[&str] = &[
    "ManeuverKnightToOutpost",
    "ManeuverBishopToSupportPoint",
    "ManeuverRookToOpenFile",
    "UndermineDefender",
    "OverprotectStrongPoint",
    "TakeOpposition",
    "CreatePassedPawn",
    "HuntBishopPair",
    "TradeSquareDefender",
    "KeepBestPiece",
    "TradeOffAttacker",
    "PressureBackwardPawn",
    "BlockadeWhitePasser",
    "BlockadeBlackPasser",
    "AdvanceQueensideMajority",
    "DoubleOnOpenFile",
    "BlockadeThenPressure",
    "KeepPositionClosed",
    "OpenPositionForBishops",
    "OpenPositionBeforeOpponentCompletes",
    "UseSpaceAvoidExchanges",
    // Run 8.5 vocabulary additions (book-trial tuning).
    "WingPawnStormClosedCenter",
    "MinorityAttack",
    "RookToSeventh",
    "RookBehindPasser",
    "PressureDoubledPawn",
    "TradeOrActivateBadBishop",
    "ActivateKingInEndgame",
    "RestrictKnight",
    "AdvanceCentralMajority",
    "OpenLinesTowardWeakKing",
    // Run 11: development-prior vocabulary (kibitz-core::development).
    "CompleteDevelopment",
    "CastleIntoSafety",
    "ClaimTheCenter",
    "QueenAheadOfHerArmy",
    "SamePieceWandering",
];

#[derive(Debug, Default)]
pub struct AxisScore {
    pub hits: u32,
    pub total: u32,
}

impl AxisScore {
    fn pct(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            100.0 * f64::from(self.hits) / f64::from(self.total)
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub positions: u32,
    pub bad_fens: Vec<(String, String)>,
    pub imbalance: AxisScore,
    pub plans: AxisScore,
    pub alerts: AxisScore,
    pub favors: AxisScore,
    /// Suggestion hit-rate (run 10): the TOP suggestion matches one of
    /// the book's best moves / ANY of the top three does. One check per
    /// entry that carries best_moves.
    pub suggest_top1: AxisScore,
    pub suggest_top3: AxisScore,
    /// expectation tag -> occurrences, for tags the engine cannot emit.
    pub vocabulary_gaps: BTreeMap<String, u32>,
    /// (entry id, axis, expected item) for every miss.
    pub misses: Vec<(String, &'static str, String)>,
    /// Precision: negative assertions that FIRED anyway.
    pub false_fires: Vec<(String, &'static str, String)>,
    pub negative_checks: u32,
}

pub fn eval_corpus(corpus: &Corpus) -> Report {
    let mut r = Report::default();
    for e in &corpus.positions {
        let Ok(board) = e.fen.parse::<cozy_chess::Board>() else {
            r.bad_fens.push((e.id.clone(), e.fen.clone()));
            continue;
        };
        r.positions += 1;
        let record = kibitz_core::analyze(&board);

        // Development prior (run 11): fed with the entry's replayed
        // history when it reaches the FEN, else with the bare position.
        // Its hints/kind extend detection only — the favors vote stays
        // untouched (a to-do list is not an advantage lean).
        let development = {
            let start = cozy_chess::Board::default();
            let mut replay = start.clone();
            let mut moves: Vec<cozy_chess::Move> = Vec::new();
            for san in &e.sans {
                let Ok(mv) = crate::san::parse_san(&replay, san) else {
                    moves.clear();
                    break;
                };
                replay.play(mv);
                moves.push(mv);
            }
            if !moves.is_empty() && replay.same_position(&board) {
                kibitz_core::development::track(&start, &moves)
            } else {
                kibitz_core::development::track(&board, &[])
            }
        };
        let development = kibitz_core::development::imbalances(&development);

        let mut detected_kinds: Vec<String> = record
            .imbalances
            .iter()
            .map(|i| format!("{:?}", i.kind))
            .collect();
        if !development.is_empty() {
            detected_kinds.push("Development".to_string());
        }
        for want in &e.expected.imbalances {
            r.imbalance.total += 1;
            if detected_kinds.iter().any(|k| k == want) {
                r.imbalance.hits += 1;
            } else {
                r.misses.push((e.id.clone(), "imbalance", want.clone()));
            }
        }

        let mut detected_hints: Vec<String> = record
            .imbalances
            .iter()
            .flat_map(|i| i.plans.iter().map(|p| p.hint.clone()))
            .collect();
        detected_hints.extend(record.composite_plans.iter().flat_map(|c| c.hints.clone()));
        detected_hints.extend(
            development
                .iter()
                .flat_map(|i| i.plans.iter().map(|p| p.hint.clone())),
        );
        // Maneuvers are plans too. A reroute that carries its own owner
        // (opposition, which belongs to the side to move) is emitted as a
        // Maneuver rather than a PlanHint precisely so it cannot be
        // misattributed — the harness has to look there as well.
        for m in &record.maneuvers {
            if let Some(token) = maneuver_token(&m.reason) {
                detected_hints.push(token.to_string());
            }
        }
        for want in &e.expected.plan_tags {
            if KNOWN_HINTS.contains(&want.as_str()) {
                r.plans.total += 1;
                if detected_hints.iter().any(|h| h == want) {
                    r.plans.hits += 1;
                } else {
                    r.misses.push((e.id.clone(), "plan", want.clone()));
                }
            } else {
                *r.vocabulary_gaps.entry(want.clone()).or_default() += 1;
            }
        }

        let detected_alerts: Vec<String> = record
            .wsui
            .alerts
            .iter()
            .map(|a| format!("{:?}", a.kind))
            .collect();
        for want in &e.expected.alerts {
            r.alerts.total += 1;
            if detected_alerts.iter().any(|k| k == want) {
                r.alerts.hits += 1;
            } else {
                r.misses.push((e.id.clone(), "alert", want.clone()));
            }
        }

        for banned in &e.not_expected.imbalances {
            r.negative_checks += 1;
            if detected_kinds.iter().any(|k| k == banned) {
                r.false_fires
                    .push((e.id.clone(), "imbalance", banned.clone()));
            }
        }
        for banned in &e.not_expected.plan_tags {
            r.negative_checks += 1;
            if detected_hints.iter().any(|h| h == banned) {
                r.false_fires.push((e.id.clone(), "plan", banned.clone()));
            }
        }

        // Suggestion hit-rate (run 10) against transcribed book moves.
        // The harness runs NO engine, so the whole-board static veto
        // governs (run 11): marked candidates are dropped exactly as a
        // no-engine consumer would drop them. This is the honest number
        // for what users see without verification — tactical book moves
        // that statics must veto now count as misses (see
        // docs/VALIDATION.md).
        if !e.expected.best_moves.is_empty() {
            let suggestions: Vec<_> = kibitz_core::suggest::suggest(&record, &board)
                .into_iter()
                .filter(|s| s.static_risk.is_none())
                .collect();
            let hit = |s: &kibitz_core::suggest::Suggestion| {
                e.expected
                    .best_moves
                    .iter()
                    .any(|bm| san_matches(&s.san, bm))
            };
            r.suggest_top1.total += 1;
            if suggestions.first().is_some_and(hit) {
                r.suggest_top1.hits += 1;
            }
            r.suggest_top3.total += 1;
            if suggestions.iter().any(hit) {
                r.suggest_top3.hits += 1;
            } else {
                r.misses.push((
                    e.id.clone(),
                    "suggest",
                    format!(
                        "{:?} (we said {:?})",
                        e.expected.best_moves,
                        suggestions
                            .iter()
                            .map(|s| s.san.clone())
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }

        if let Some(want) = e.expected.favors.as_deref() {
            if want != "balanced" {
                r.favors.total += 1;
                // Magnitude-weighted lean across detected imbalances.
                let mut lean = 0i32;
                for i in &record.imbalances {
                    let w = match i.magnitude {
                        kibitz_core::record::Magnitude::Minor => 1,
                        kibitz_core::record::Magnitude::Clear => 2,
                        kibitz_core::record::Magnitude::Winning => 4,
                    };
                    match i.favors {
                        kibitz_core::record::Favors::White => lean += w,
                        kibitz_core::record::Favors::Black => lean -= w,
                        kibitz_core::record::Favors::Balanced => {}
                    }
                }
                let ours = if lean > 0 {
                    "white"
                } else if lean < 0 {
                    "black"
                } else {
                    "balanced"
                };
                if ours == want {
                    r.favors.hits += 1;
                } else {
                    r.misses
                        .push((e.id.clone(), "favors", format!("{want} (we said {ours})")));
                }
            }
        }
    }
    r
}

/// SAN comparison tolerant of decoration: check/mate/annotation marks are
/// stripped from both sides before comparing.
fn san_matches(ours: &str, book: &str) -> bool {
    fn strip(s: &str) -> &str {
        s.trim_end_matches(['+', '#', '!', '?'])
    }
    strip(ours) == strip(book)
}

/// Load every *.json corpus under `path` (file or directory).
pub fn load(path: &Path) -> anyhow::Result<Vec<Corpus>> {
    let mut out = Vec::new();
    let files: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };
    for f in files {
        let corpus: Corpus = serde_json::from_str(&std::fs::read_to_string(&f)?)
            .map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
        out.push(corpus);
    }
    Ok(out)
}

pub fn print_report(book: &str, r: &Report, verbose: bool) {
    println!("== {book} — {} positions", r.positions);
    if !r.bad_fens.is_empty() {
        println!("  UNPARSEABLE FENS: {}", r.bad_fens.len());
        for (id, fen) in &r.bad_fens {
            println!("    {id}: {fen}");
        }
    }
    let line = |name: &str, s: &AxisScore| {
        if s.total > 0 {
            println!(
                "  {name:<12} {:>3}/{:<3} = {:>5.1}%",
                s.hits,
                s.total,
                s.pct()
            );
        }
    };
    line("imbalances", &r.imbalance);
    line("plans", &r.plans);
    line("alerts", &r.alerts);
    line("favors", &r.favors);
    line("suggest@1", &r.suggest_top1);
    line("suggest@3", &r.suggest_top3);
    if !r.vocabulary_gaps.is_empty() {
        println!("  vocabulary gaps (no matching hint yet):");
        for (tag, n) in &r.vocabulary_gaps {
            println!("    {tag} ×{n}");
        }
    }
    if r.negative_checks > 0 {
        println!(
            "  negatives    {}/{} clean",
            r.negative_checks - r.false_fires.len() as u32,
            r.negative_checks
        );
        for (id, axis, what) in &r.false_fires {
            println!("  FALSE-FIRE {axis:<9} {id}: {what}");
        }
    }
    if verbose {
        for (id, axis, want) in &r.misses {
            println!("  MISS {axis:<9} {id}: expected {want}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic corpus (no book content): the Sveshnikov bind position
    /// our golden tests already cover must score on all axes.
    #[test]
    fn harness_scores_a_known_position() {
        let corpus = Corpus {
            book: "synthetic".into(),
            edition: String::new(),
            positions: vec![
                Entry {
                    id: "syn-1".into(),
                    citation: String::new(),
                    sans: vec![],
                    fen: "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7"
                        .into(),
                    kind: "example".into(),
                    confidence: "high".into(),
                    not_expected: NotExpected::default(),
                    expected: Expected {
                        favors: Some("white".into()),
                        imbalances: vec!["SquaresOutposts".into(), "PawnStructure".into()],
                        plan_tags: vec![
                            "ManeuverKnightToOutpost".into(),
                            "minority-attack".into(), // vocabulary gap
                        ],
                        alerts: vec![],
                        theme: String::new(),
                        best_moves: vec!["Nd5".into()],
                    },
                },
                Entry {
                    id: "syn-bad-fen".into(),
                    citation: String::new(),
                    sans: vec![],
                    fen: "not a fen".into(),
                    kind: "example".into(),
                    confidence: "high".into(),
                    not_expected: NotExpected::default(),
                    expected: Expected::default(),
                },
            ],
        };
        let r = eval_corpus(&corpus);
        assert_eq!(r.positions, 1);
        assert_eq!(r.bad_fens.len(), 1);
        assert_eq!(r.imbalance.total, 2);
        assert_eq!(r.imbalance.hits, 2, "misses: {:?}", r.misses);
        assert_eq!(r.plans.total, 1);
        assert_eq!(r.plans.hits, 1);
        assert_eq!(r.vocabulary_gaps.get("minority-attack"), Some(&1));
        assert_eq!(r.favors.hits, 1);
        // Run 10: the Sveshnikov bind's book move Nd5 is our suggestion.
        assert_eq!(r.suggest_top3.total, 1);
        assert_eq!(r.suggest_top3.hits, 1, "misses: {:?}", r.misses);
    }
}
