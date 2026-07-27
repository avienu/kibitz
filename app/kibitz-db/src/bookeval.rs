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
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub expected: Expected,
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
}

/// Known PlanHint tokens the engine can emit today. Expected plan tags
/// outside this list are vocabulary gaps, reported rather than scored.
const KNOWN_HINTS: &[&str] = &[
    "ManeuverKnightToOutpost",
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
    /// expectation tag -> occurrences, for tags the engine cannot emit.
    pub vocabulary_gaps: BTreeMap<String, u32>,
    /// (entry id, axis, expected item) for every miss.
    pub misses: Vec<(String, &'static str, String)>,
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

        let detected_kinds: Vec<String> = record
            .imbalances
            .iter()
            .map(|i| format!("{:?}", i.kind))
            .collect();
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
    if !r.vocabulary_gaps.is_empty() {
        println!("  vocabulary gaps (no matching hint yet):");
        for (tag, n) in &r.vocabulary_gaps {
            println!("    {tag} ×{n}");
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
                    fen: "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7"
                        .into(),
                    kind: "example".into(),
                    confidence: "high".into(),
                    expected: Expected {
                        favors: Some("white".into()),
                        imbalances: vec!["SquaresOutposts".into(), "PawnStructure".into()],
                        plan_tags: vec![
                            "ManeuverKnightToOutpost".into(),
                            "minority-attack".into(), // vocabulary gap
                        ],
                        alerts: vec![],
                        theme: String::new(),
                    },
                },
                Entry {
                    id: "syn-bad-fen".into(),
                    citation: String::new(),
                    fen: "not a fen".into(),
                    kind: "example".into(),
                    confidence: "high".into(),
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
    }
}
