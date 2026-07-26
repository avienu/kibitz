//! Snapshot, lint and property tests for template-mode verbalization.
//!
//! The hand-built records cover: a tactical position (high-severity
//! InadequatelyDefended with a confirmed engine check, plus a loose rook),
//! a quiet positional middlegame (three imbalances, minor one dropped),
//! an endgame with a passed pawn, an empty record, the maintainer's
//! bad-bishop complaint case, and a synthetic record that exercises every
//! evidence key and alert detail the silman-core detectors emit.
//!
//! Two hard guarantees are enforced across all of them:
//! - the prose lint: no underscores, brackets, braces, quotes, or bare
//!   labeled numbers may ever reach the output (no serialized data leaks);
//! - the no-invention property: every square mentioned in the prose appears
//!   somewhere in the record's own data.

use std::collections::{BTreeMap, BTreeSet};

use silman_core::record::{
    AlertKind, EngineCheck, EngineCheckStatus, EngineEval, Favors, FeatureRecord, Imbalance,
    ImbalanceKind, Magnitude, Phase, PlanHint, Provenance, Severity, SideColor, TacticAlert,
    WsuiReport, SCHEMA_VERSION,
};
use silman_verbalize::{verbalize, verbalize_sections};

fn provenance() -> Provenance {
    Provenance {
        generator: "silman-core".into(),
        version: "0.1.0".into(),
    }
}

fn base_record(fen: &str, phase: Phase) -> FeatureRecord {
    FeatureRecord {
        schema_version: SCHEMA_VERSION,
        fen: fen.into(),
        side_to_move: SideColor::White,
        phase,
        wsui: WsuiReport {
            alerts: vec![],
            screen_fired: false,
        },
        imbalances: vec![],
        engine: None,
        provenance: provenance(),
    }
}

fn evidence(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

/// (a) Tactical: Black's c6-knight attacked by Ne5 and Bb5, held only by
/// the b7-pawn (engine-confirmed), plus a loose rook on a8.
fn tactical_record() -> FeatureRecord {
    let mut record = base_record(
        "r1bqk2r/pp2bppp/2n2n2/1B1pN3/3P4/2N5/PPP2PPP/R1BQK2R w KQkq - 0 9",
        Phase::Middlegame,
    );
    record.wsui = WsuiReport {
        alerts: vec![
            TacticAlert {
                kind: AlertKind::InadequatelyDefended,
                side: SideColor::Black,
                target: Some("c6".into()),
                attackers: vec!["e5".into(), "b5".into()],
                defenders: vec!["b7".into()],
                see: Some(200),
                severity: Severity::High,
                detail: Some("overloaded-defender".into()),
                engine_check: Some(EngineCheck {
                    status: EngineCheckStatus::Confirmed,
                    pv: vec!["Bxc6".into(), "bxc6".into(), "Nxc6".into()],
                    score_delta_cp: Some(190),
                    budget_nodes: 2_000_000,
                }),
            },
            TacticAlert {
                kind: AlertKind::Undefended,
                side: SideColor::Black,
                target: Some("a8".into()),
                attackers: vec![],
                defenders: vec![],
                see: None,
                severity: Severity::Medium,
                detail: None,
                engine_check: None,
            },
        ],
        screen_fired: true,
    };
    record
}

/// (b) Quiet positional middlegame: three imbalances with detector-real
/// evidence keys; the minor one (Black's e4 outpost) must be dropped by
/// the 3+ rule, and its plan with it.
fn quiet_record() -> FeatureRecord {
    let mut record = base_record(
        "r2q1rk1/pp3ppp/2n2n2/3p4/8/2NBPN2/PPP2PPP/R1BQ1RK1 w - - 0 11",
        Phase::Middlegame,
    );
    record.imbalances = vec![
        Imbalance {
            kind: ImbalanceKind::PawnStructure,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[("isolated_black", serde_json::json!(["d5"]))]),
            plans: vec![PlanHint {
                hint: "BlockadeThenPressure".into(),
                squares: vec!["d4".into(), "d5".into()],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::MinorPieces,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("bishop_pair", serde_json::json!("white")),
                ("character", serde_json::json!("open")),
                ("locked_center_pawns", serde_json::json!(0)),
            ]),
            plans: vec![PlanHint {
                hint: "OpenThePosition".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::SquaresOutposts,
            favors: Favors::Black,
            magnitude: Magnitude::Minor,
            evidence: evidence(&[("established_outpost_black", serde_json::json!("e4"))]),
            plans: vec![PlanHint {
                hint: "ManeuverKnight".into(),
                squares: vec!["f6".into(), "e4".into()],
            }],
        },
    ];
    record
}

/// (c) King-and-pawn endgame with an outside passed a-pawn, a balanced
/// material note, and an overall engine eval.
fn endgame_record() -> FeatureRecord {
    let mut record = base_record("8/5pk1/6p1/P7/8/6P1/5PK1/8 w - - 0 40", Phase::Endgame);
    record.imbalances = vec![
        Imbalance {
            kind: ImbalanceKind::PawnStructure,
            favors: Favors::White,
            magnitude: Magnitude::Winning,
            evidence: evidence(&[("passed_white", serde_json::json!(["a5"]))]),
            plans: vec![PlanHint {
                hint: "EscortThePasser".into(),
                squares: vec!["a5".into(), "a6".into(), "a7".into(), "a8".into()],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::Material,
            favors: Favors::Balanced,
            magnitude: Magnitude::Minor,
            evidence: evidence(&[("material_diff_cp", serde_json::json!(0))]),
            plans: vec![],
        },
    ];
    record.engine = Some(EngineEval {
        eval_cp: 350,
        best: "a6".into(),
        multipv: vec![],
    });
    record
}

/// (d) Nothing to report at all.
fn empty_record() -> FeatureRecord {
    base_record(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        Phase::Opening,
    )
}

/// (e) The maintainer's complaint case, verbatim evidence: a bad black
/// bishop object, an "open" character tag and a zero locked-pawn count must
/// come out as clean prose, not serialized data. Also carries the alert
/// shape behind the "Only ... keeps it defended" agreement bug (two
/// defenders) and a plain material edge for the pawns-equivalent phrasing.
fn maintainer_case_record() -> FeatureRecord {
    let mut record = base_record(
        "4kb1r/8/3p4/2p5/8/8/5PPP/2B1K2R w Kk - 0 25",
        Phase::Middlegame,
    );
    record.wsui = WsuiReport {
        alerts: vec![TacticAlert {
            kind: AlertKind::InadequatelyDefended,
            side: SideColor::Black,
            target: Some("f8".into()),
            attackers: vec![],
            defenders: vec!["e8".into(), "h8".into()],
            see: None,
            severity: Severity::Low,
            detail: None,
            engine_check: None,
        }],
        screen_fired: false,
    };
    record.imbalances = vec![
        Imbalance {
            kind: ImbalanceKind::MinorPieces,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                (
                    "bad_bishop_black",
                    serde_json::json!({"bishop": "f8", "blocking_pawns": ["c5", "d6"]}),
                ),
                ("character", serde_json::json!("open")),
                ("locked_center_pawns", serde_json::json!(0)),
            ]),
            plans: vec![],
        },
        Imbalance {
            kind: ImbalanceKind::Material,
            favors: Favors::White,
            magnitude: Magnitude::Minor,
            evidence: evidence(&[("material_diff_cp", serde_json::json!(100))]),
            plans: vec![],
        },
    ];
    record
}

/// (f) Synthetic full-coverage record: every evidence key the silman-core
/// detectors emit appears at least once, plus every alert detail string
/// (including a compound WeakKing detail). Deliberately overloaded — its
/// only job is to prove that no field name, bracket or bare count can
/// survive into the prose.
fn full_coverage_record() -> FeatureRecord {
    let mut record = base_record(
        "r5k1/1p6/2n5/NB5b/6P1/8/2Q5/3R2K1 w - - 0 30",
        Phase::Middlegame,
    );
    record.wsui = WsuiReport {
        alerts: vec![
            TacticAlert {
                kind: AlertKind::WeakKing,
                side: SideColor::Black,
                target: Some("g8".into()),
                attackers: vec!["c2".into(), "d1".into()],
                defenders: vec![],
                see: None,
                severity: Severity::High,
                detail: Some(
                    "zone-pressure+3; f-file shield pawn missing; g-file shield pawn missing; \
                     h-file shield pawn advanced; open-files:g; back-rank"
                        .into(),
                ),
                engine_check: None,
            },
            TacticAlert {
                kind: AlertKind::TrappedPiece,
                side: SideColor::Black,
                target: Some("h5".into()),
                attackers: vec!["g4".into()],
                defenders: vec![],
                see: Some(300),
                severity: Severity::High,
                detail: Some("trapped-and-attacked".into()),
                engine_check: None,
            },
            TacticAlert {
                kind: AlertKind::Undefended,
                side: SideColor::Black,
                target: Some("c6".into()),
                attackers: vec!["b5".into()],
                defenders: vec![],
                see: None,
                severity: Severity::Medium,
                detail: Some("attacked-and-undefended".into()),
                engine_check: None,
            },
            TacticAlert {
                kind: AlertKind::InadequatelyDefended,
                side: SideColor::Black,
                target: Some("b7".into()),
                attackers: vec![],
                defenders: vec!["c6".into(), "a8".into()],
                see: None,
                severity: Severity::Medium,
                detail: Some("overloaded-defender".into()),
                engine_check: None,
            },
            TacticAlert {
                kind: AlertKind::TrappedPiece,
                side: SideColor::White,
                target: Some("a5".into()),
                attackers: vec![],
                defenders: vec![],
                see: None,
                severity: Severity::Medium,
                detail: Some("trapped-attackable".into()),
                engine_check: None,
            },
            TacticAlert {
                kind: AlertKind::Undefended,
                side: SideColor::Black,
                target: Some("a8".into()),
                attackers: vec![],
                defenders: vec![],
                see: None,
                severity: Severity::Low,
                detail: Some("loose".into()),
                engine_check: None,
            },
        ],
        screen_fired: true,
    };
    record.imbalances = vec![
        Imbalance {
            kind: ImbalanceKind::MinorPieces,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("bishop_pair", serde_json::json!("white")),
                (
                    "bad_bishop_white",
                    serde_json::json!({"bishop": "g2", "blocking_pawns": ["e4", "f3"]}),
                ),
                (
                    "bad_bishop_black",
                    serde_json::json!({"bishop": "f8", "blocking_pawns": ["c5", "d6"]}),
                ),
                ("character", serde_json::json!("closed")),
                ("locked_center_pawns", serde_json::json!(3)),
            ]),
            plans: vec![PlanHint {
                hint: "KeepPositionClosed".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::PawnStructure,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("isolated_white", serde_json::json!(["a4"])),
                ("isolated_black", serde_json::json!(["h5"])),
                ("doubled_white", serde_json::json!(["c3", "c4"])),
                ("doubled_black", serde_json::json!(["f7", "f5"])),
                ("backward_white", serde_json::json!(["d3"])),
                ("backward_black", serde_json::json!(["d6"])),
                ("passed_white", serde_json::json!(["b5"])),
                ("passed_black", serde_json::json!(["h4"])),
                ("queenside_majority", serde_json::json!("white")),
                ("kingside_majority", serde_json::json!("black")),
            ]),
            plans: vec![
                PlanHint {
                    hint: "AdvanceQueensideMajority".into(),
                    squares: vec![],
                },
                PlanHint {
                    hint: "BlockadeBlackPasser".into(),
                    squares: vec!["h3".into()],
                },
            ],
        },
        Imbalance {
            kind: ImbalanceKind::Material,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("material_diff_cp", serde_json::json!(180)),
                ("pattern", serde_json::json!("white-exchange-up")),
            ]),
            plans: vec![],
        },
        Imbalance {
            kind: ImbalanceKind::FilesDiagonals,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("open_files", serde_json::json!(["d", "e"])),
                ("half_open_files_white", serde_json::json!(["c"])),
                ("half_open_files_black", serde_json::json!(["g", "h"])),
                ("doubled_majors_d", serde_json::json!("white")),
                ("rook_on_seventh", serde_json::json!("white")),
            ]),
            plans: vec![PlanHint {
                hint: "DoubleOnOpenFile".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::SquaresOutposts,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("holes_in_black_camp", serde_json::json!(["d5", "e5"])),
                ("holes_in_white_camp", serde_json::json!(["e4"])),
                ("established_outpost_white", serde_json::json!("d5")),
                ("established_outpost_black", serde_json::json!("e4")),
            ]),
            plans: vec![],
        },
        Imbalance {
            kind: ImbalanceKind::Space,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("white_space", serde_json::json!(8)),
                ("black_space", serde_json::json!(3)),
            ]),
            plans: vec![PlanHint {
                hint: "UseSpaceAvoidExchanges".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::Development,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("white_developed", serde_json::json!(5)),
                ("black_developed", serde_json::json!(2)),
            ]),
            plans: vec![PlanHint {
                hint: "OpenPositionBeforeOpponentCompletes".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::Initiative,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: evidence(&[
                ("white_forcing_moves", serde_json::json!(6)),
                ("black_forcing_moves", serde_json::json!(1)),
            ]),
            plans: vec![],
        },
    ];
    record
}

fn all_records() -> Vec<FeatureRecord> {
    vec![
        tactical_record(),
        quiet_record(),
        endgame_record(),
        empty_record(),
        maintainer_case_record(),
        full_coverage_record(),
    ]
}

#[test]
fn tactical_prose() {
    let out = verbalize(&tactical_record());
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn quiet_positional_prose() {
    let record = quiet_record();
    let sections = verbalize_sections(&record);
    assert!(sections.tactics.is_empty());
    // The minor outpost imbalance and its plan must have been dropped.
    assert!(!sections.imbalances.contains("e4"));
    assert!(!sections.plans.contains("knight"));
    let out = verbalize(&record);
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn endgame_prose() {
    let out = verbalize(&endgame_record());
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn empty_record_prose() {
    let out = verbalize(&empty_record());
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn maintainer_case_prose() {
    let out = verbalize(&maintainer_case_record());
    lint_prose(&out);
    // The exact leaks the maintainer flagged must be gone.
    for leaked in [
        "blocking_pawns",
        "locked center pawns",
        "character:",
        "bad bishop black",
    ] {
        assert!(!out.contains(leaked), "leak {leaked:?} in:\n{out}");
    }
    // Subject-verb agreement with two defenders.
    assert!(out.contains("keep it defended"), "agreement bug in:\n{out}");
    assert!(
        !out.contains("keeps it defended"),
        "agreement bug in:\n{out}"
    );
    insta::assert_snapshot!(out);
}

#[test]
fn full_coverage_prose() {
    let out = verbalize(&full_coverage_record());
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

/// THE LINT: rendered prose may never contain serialized-data residue —
/// underscores, brackets, braces, quotes, or a bare number introduced by
/// ": " (square names like "a5" are fine; they never start with a digit).
fn lint_prose(text: &str) {
    for forbidden in ['_', '[', ']', '{', '}', '"'] {
        assert!(
            !text.contains(forbidden),
            "forbidden character {forbidden:?} in output:\n{text}"
        );
    }
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b':' && bytes[i + 1] == b' ' {
            assert!(
                !bytes[i + 2].is_ascii_digit(),
                "bare labeled number after ':' in output:\n{text}"
            );
        }
    }
}

/// Run the lint over every record's full rendering and every section.
#[test]
fn prose_lint_over_all_records() {
    for record in all_records() {
        lint_prose(&verbalize(&record));
        let sections = verbalize_sections(&record);
        lint_prose(&sections.tactics);
        lint_prose(&sections.imbalances);
        lint_prose(&sections.plans);
    }
}

/// Run the lint over the template texts themselves ({slot} spans removed),
/// so no template can smuggle serialized residue in even before rendering.
/// `evidence.order` is machine configuration, not user-visible text.
#[test]
fn prose_lint_over_all_templates() {
    let sources = [
        include_str!("../templates/common.tmpl"),
        include_str!("../templates/alerts.tmpl"),
        include_str!("../templates/imbalances.tmpl"),
        include_str!("../templates/evidence.tmpl"),
        include_str!("../templates/plans.tmpl"),
    ];
    for source in sources {
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "evidence.order" {
                continue;
            }
            let mut stripped = String::new();
            let mut in_slot = false;
            for c in value.chars() {
                match c {
                    '{' => in_slot = true,
                    '}' => in_slot = false,
                    _ if !in_slot => stripped.push(c),
                    _ => {}
                }
            }
            lint_prose(&stripped);
        }
    }
}

/// Every substring shaped like a board square ([a-h][1-8]).
fn squares_in(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .windows(2)
        .filter(|pair| ('a'..='h').contains(&pair[0]) && ('1'..='8').contains(&pair[1]))
        .map(|pair| pair.iter().collect())
        .collect()
}

/// The hard property template mode satisfies by construction: no square is
/// ever mentioned in the prose unless it appears in the record's own data
/// (alert squares, PVs, evidence, plan squares, engine moves). The FEN is
/// blanked before extracting the allowed set so it cannot mask inventions.
#[test]
fn output_squares_all_come_from_the_record() {
    for record in all_records() {
        let out = verbalize(&record);
        let mut data_only = record.clone();
        data_only.fen = String::new();
        let allowed = squares_in(&serde_json::to_string(&data_only).unwrap());
        let used = squares_in(&out);
        let invented: Vec<&String> = used.difference(&allowed).collect();
        assert!(
            invented.is_empty(),
            "verbalizer invented squares {invented:?} in:\n{out}"
        );
    }
}
