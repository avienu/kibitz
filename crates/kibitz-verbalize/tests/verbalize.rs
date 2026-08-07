//! Snapshot, lint and property tests for template-mode verbalization.
//!
//! The hand-built records cover: a tactical position (high-severity
//! InadequatelyDefended with a confirmed engine check, plus a loose rook),
//! a quiet positional middlegame (three imbalances, minor one dropped),
//! an endgame with a passed pawn, an empty record, the maintainer's
//! bad-bishop complaint case, and a synthetic record that exercises every
//! evidence key and alert detail the kibitz-core detectors emit.
//!
//! Two hard guarantees are enforced across all of them, and across BOTH
//! voices (run-5 item 3 — the Coach overlay and the Neutral baseline):
//! - the prose lint: no underscores, brackets, braces, quotes, or bare
//!   labeled numbers may ever reach the output (no serialized data leaks);
//! - the no-invention property: every square mentioned in the prose appears
//!   somewhere in the record's own data.

use std::collections::{BTreeMap, BTreeSet};

use kibitz_core::record::{
    AlertKind, EngineCheck, EngineCheckStatus, EngineEval, Favors, FeatureRecord, Imbalance,
    ImbalanceKind, Magnitude, Phase, PlanHint, Provenance, Severity, SideColor, TacticAlert,
    WsuiReport, SCHEMA_VERSION,
};
use kibitz_verbalize::{
    verbalize, verbalize_sections, verbalize_sections_voiced, verbalize_voiced, Voice,
};

fn provenance() -> Provenance {
    Provenance {
        generator: "kibitz-core".into(),
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
        composite_plans: vec![],
        maneuvers: vec![],
        schemes: vec![],
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
                    mate_in: None,
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
        mate_in: None,
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

/// (f) Synthetic full-coverage record: every evidence key the kibitz-core
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

/// (g) Development prior (run 11): a REAL record with history — the
/// scholar's-mate-adjacent 1.e4 e5 2.Qh5, White's queen ahead of her
/// sleeping army — exercising the prior evidence keys, the to-do
/// headline, and the misplay observations end to end.
fn development_prior_record() -> FeatureRecord {
    let start = kibitz_core::cozy_chess::Board::default();
    let moves: Vec<kibitz_core::cozy_chess::Move> = ["e2e4", "e7e5", "d1h5"]
        .iter()
        .map(|u| u.parse().unwrap())
        .collect();
    kibitz_core::analyze_with_history(&start, &moves)
}

fn all_records() -> Vec<FeatureRecord> {
    vec![
        tactical_record(),
        quiet_record(),
        endgame_record(),
        empty_record(),
        maintainer_case_record(),
        full_coverage_record(),
        development_prior_record(),
    ]
}

#[test]
fn tactical_prose() {
    let out = verbalize(&tactical_record());
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

/// Per-voice snapshot for the tactical record (Coach is snapshotted by
/// `tactical_prose` above, since Coach is the default voice).
#[test]
fn tactical_prose_neutral() {
    let out = verbalize_voiced(&tactical_record(), Voice::Neutral);
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

/// Per-voice snapshot for the quiet record (Coach is the default above).
#[test]
fn quiet_positional_prose_neutral() {
    let record = quiet_record();
    let sections = verbalize_sections_voiced(&record, Voice::Neutral);
    assert!(sections.tactics.is_empty());
    assert!(!sections.imbalances.contains("e4"));
    assert!(!sections.plans.contains("knight"));
    let out = verbalize_voiced(&record, Voice::Neutral);
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

/// Run 11: the development prior speaks as dreams-under-uncertainty in
/// Coach and as plain facts in Neutral — never as a numbered rulebook.
#[test]
fn development_prior_prose() {
    let record = development_prior_record();
    let coach = verbalize_voiced(&record, Voice::Coach);
    let neutral = verbalize_voiced(&record, Voice::Neutral);
    lint_prose(&coach);
    lint_prose(&neutral);
    // The prior's stories are present in both voices.
    for out in [&coach, &neutral] {
        let lower = out.to_lowercase();
        assert!(lower.contains("queen"), "queen sortie missing: {out}");
        assert!(
            lower.contains("develop") || lower.contains("dreaming") || lower.contains("at home"),
            "development story missing: {out}"
        );
        // Never rulebook phrasing.
        assert!(!lower.contains("principle"), "{out}");
        assert!(!lower.contains("rule"), "{out}");
    }
    // Coach voices the maintainer's framing on the mixed sleeper list.
    assert!(
        coach.contains("the knights already know where they are going"),
        "{coach}"
    );
    insta::assert_snapshot!(coach);
}

#[test]
fn development_prior_prose_neutral() {
    let out = verbalize_voiced(&development_prior_record(), Voice::Neutral);
    lint_prose(&out);
    insta::assert_snapshot!(out);
}

/// Run 11: the book line renders in both voices, and explain_in_book
/// places it as the headline of a silent position or as a trailing block
/// otherwise. Book state is purely the caller's flag.
#[test]
fn book_line_and_explain_in_book() {
    use kibitz_verbalize::{book_line, explain_in_book};
    let coach = book_line(Voice::Coach);
    let neutral = book_line(Voice::Neutral);
    assert!(coach.contains("book"), "{coach}");
    assert!(neutral.contains("book"), "{neutral}");
    assert_ne!(coach, neutral);
    lint_prose(&coach);
    lint_prose(&neutral);

    // A silent record: the book line becomes the headline.
    let silent = empty_record();
    let e = explain_in_book(&silent, true);
    assert_eq!(e.headline.coach, coach);
    assert_eq!(e.headline.neutral, neutral);

    // A talkative record: the line is one trailing block, evidence-free.
    let busy = quiet_record();
    let e = explain_in_book(&busy, true);
    let last = e.blocks.last().expect("blocks");
    assert_eq!(last.text.coach, coach);
    assert_eq!(last.evidence, Default::default());
    // And the flag off changes nothing.
    let plain = explain_in_book(&busy, false);
    assert_eq!(plain, kibitz_verbalize::explain(&busy));
}

/// Run 10: the candidate-move closing sentence — rendered on demand (the
/// narrator appends it at plans-narrated, non-capture plies), in both
/// voices, lint-clean, prophylactic when the top pick denies the
/// opponent's plan. Gated off by mate/decisive engine lines.
#[test]
fn suggestion_closing_sentence() {
    use kibitz_core::record::EngineEval;
    use kibitz_verbalize::suggestion_closing;
    let record = quiet_record();
    // On the quiet record's own board the top pick (e4) DENIES Black's
    // knight-to-e4 plan: the coach says so, in both voices.
    let coach = suggestion_closing(&record, Voice::Coach).expect("closing");
    let neutral = suggestion_closing(&record, Voice::Neutral).expect("closing");
    lint_prose(&coach);
    lint_prose(&neutral);
    assert!(coach.contains("deny the opponent"), "{coach}");
    assert!(coach.contains("e4"), "{coach}");
    assert!(neutral.contains("deny the opponent"), "{neutral}");
    assert_ne!(coach, neutral);

    // A decisive or mate engine line silences the closing.
    let mut decided = quiet_record();
    decided.engine = Some(EngineEval {
        eval_cp: 910,
        mate_in: None,
        best: "g4".into(),
        multipv: vec![],
    });
    assert_eq!(suggestion_closing(&decided, Voice::Coach), None);
    let mut mate = quiet_record();
    mate.engine = Some(EngineEval {
        eval_cp: 31_900,
        mate_in: Some(5),
        best: "Qh7#".into(),
        multipv: vec![],
    });
    assert_eq!(suggestion_closing(&mate, Voice::Coach), None);

    // A record with no mapped plans yields no closing at all.
    assert_eq!(suggestion_closing(&empty_record(), Voice::Coach), None);
}

/// Run 11: the closing respects the whole-board static veto and the
/// engine-verification context. French Winawer after 5.a3 (maintainer
/// field report): every static candidate is marked (the b4-bishop hangs
/// to axb4), so without an engine there is NO closing; a cleared list
/// from the bounded engine review resurrects exactly its members.
#[test]
fn suggestion_closing_respects_verification() {
    use kibitz_verbalize::{suggestion_closing, suggestion_closing_verified};
    const WINAWER: &str = "rnbqk1nr/pp3ppp/4p3/2ppP3/1b1P4/P1N5/1PP2PPP/R1BQKBNR b KQkq - 0 5";
    let board: kibitz_core::cozy_chess::Board = WINAWER.parse().unwrap();
    let record = kibitz_core::analyze(&board);
    // Sanity: the mappers DO propose moves here (they shipped as chips),
    // and every one of them carries the static mark.
    let raw = kibitz_core::suggest::suggest(&record, &board);
    assert!(!raw.is_empty());
    assert!(raw.iter().all(|s| s.static_risk.is_some()), "{raw:?}");
    // No engine: the static veto drops every marked candidate — silence.
    assert_eq!(suggestion_closing(&record, Voice::Coach), None);
    assert_eq!(
        suggestion_closing_verified(&record, Voice::Coach, None),
        None
    );
    // Engine cleared cxd4 (the theory move): only it renders.
    let cleared = vec!["c5d4".to_string()];
    let closing = suggestion_closing_verified(&record, Voice::Coach, Some(&cleared))
        .expect("cleared move renders");
    assert!(closing.contains("cxd4"), "{closing}");
    assert!(
        !closing.contains("f5") && !closing.contains("f6"),
        "{closing}"
    );
    // Engine refuted everything: an empty cleared list silences it too.
    assert_eq!(
        suggestion_closing_verified(&record, Voice::Coach, Some(&[])),
        None
    );
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

/// Run the lint over every record's full rendering and every section, in
/// BOTH voices.
#[test]
fn prose_lint_over_all_records() {
    for voice in Voice::ALL {
        for record in all_records() {
            lint_prose(&verbalize_voiced(&record, voice));
            let sections = verbalize_sections_voiced(&record, voice);
            lint_prose(&sections.tactics);
            lint_prose(&sections.imbalances);
            lint_prose(&sections.plans);
        }
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
        include_str!("../templates/coach.tmpl"),
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

/// The hard property template mode satisfies by construction, in BOTH
/// voices: no square is ever mentioned in the prose unless it appears in
/// the record's own data (alert squares, PVs, evidence, plan squares,
/// engine moves). The FEN is blanked before extracting the allowed set so
/// it cannot mask inventions. (The run-10 suggestion closing is rendered
/// on demand, outside these sections; its moves are legal moves computed
/// from the record's own FEN — record facts by construction.)
#[test]
fn output_squares_all_come_from_the_record() {
    for voice in Voice::ALL {
        for record in all_records() {
            let out = verbalize_voiced(&record, voice);
            let mut data_only = record.clone();
            data_only.fen = String::new();
            let allowed = squares_in(&serde_json::to_string(&data_only).unwrap());
            let used = squares_in(&out);
            let invented: Vec<&String> = used.difference(&allowed).collect();
            assert!(
                invented.is_empty(),
                "verbalizer ({voice:?}) invented squares {invented:?} in:\n{out}"
            );
        }
    }
}

/// Run-5 bug 1: the full mate/score rendering matrix. A mate score must
/// NEVER render in pawn units.
#[test]
fn mate_scores_never_render_as_material() {
    use kibitz_core::record::{CompositePlan, EngineEval};
    let mk = |mate_in: Option<i32>, delta: Option<i32>| {
        let mut r = tactical_record();
        if let Some(a) = r.wsui.alerts.first_mut() {
            if let Some(c) = a.engine_check.as_mut() {
                c.mate_in = mate_in;
                c.score_delta_cp = delta;
            }
        }
        verbalize(&r)
    };
    // Positive cp: pawns wording allowed.
    let cp = mk(None, Some(230));
    assert!(cp.contains("pawns"), "{cp}");
    // Mate for the beneficiary.
    let m3 = mk(Some(3), None);
    assert!(m3.contains("forced mate in 3"), "{m3}");
    assert!(
        !m3.contains("winning about"),
        "no material units for a mate: {m3}"
    );
    // Defensive guard: even if a cp sentinel sneaks in beside the mate,
    // the mate wording wins.
    let m_and_cp = mk(Some(2), Some(10_000));
    assert!(m_and_cp.contains("forced mate in 2"), "{m_and_cp}");
    assert!(!m_and_cp.contains("100 pawns"), "{m_and_cp}");
    // Mate against.
    let against = mk(Some(-2), None);
    assert!(against.contains("mates in 2"), "{against}");
    assert!(!against.contains("winning about"), "{against}");
    // Mate on the board.
    let now = mk(Some(0), None);
    assert!(now.to_lowercase().contains("checkmate"), "{now}");

    // Whole-position eval with a mate.
    let mut r = quiet_record();
    r.engine = Some(EngineEval {
        eval_cp: 10_000,
        mate_in: Some(5),
        best: "Qh7+".into(),
        multipv: vec![],
    });
    let out = verbalize(&r);
    assert!(out.contains("forced mate in 5"), "{out}");
    assert!(!out.contains("+100"), "{out}");
    let _ = CompositePlan {
        target: String::new(),
        hints: vec![],
        supporting: vec![],
        squares: vec![],
        score: 0,
        favors: kibitz_core::record::Favors::White,
    };
}

/// Run-5 item 4: a composite plan narrates as ONE unified sentence and
/// its member hints are not repeated as singles.
#[test]
fn composite_plan_narrates_unified() {
    use kibitz_core::record::{CompositePlan, Favors, ImbalanceKind};
    let mut r = quiet_record();
    r.composite_plans = vec![CompositePlan {
        target: "d5".into(),
        hints: vec![
            "ManeuverKnightToOutpost".into(),
            "PressureBackwardPawn".into(),
        ],
        supporting: vec![ImbalanceKind::SquaresOutposts, ImbalanceKind::PawnStructure],
        squares: vec!["d5".into(), "d6".into()],
        score: 4,
        favors: Favors::White,
    }];
    // Coach (default) voice: the coach lead, with the same member clauses.
    let out = verbalize(&r);
    assert!(
        out.contains("The whole position is pointing at d5"),
        "unified coach lead: {out}"
    );
    assert!(out.contains("reroute the knight"), "{out}");
    assert!(out.contains("backward pawn"), "{out}");
    // Neutral voice: the base lead, same clauses, same target.
    let neutral = verbalize_voiced(&r, Voice::Neutral);
    assert!(
        neutral.contains("Everything points to d5"),
        "unified neutral lead: {neutral}"
    );
    assert!(neutral.contains("reroute the knight"), "{neutral}");
    // Prose lints still hold in the composite path, in both voices.
    for text in [&out, &neutral] {
        for ch in ['_', '[', ']', '{', '}', '"'] {
            assert!(!text.contains(ch), "lint char {ch:?} in: {text}");
        }
    }
}

/// Run-5 item 3: the Coach voice is a pure overlay. Where an override
/// exists the voices differ; where none exists they render identically;
/// and the setting tokens round-trip.
#[test]
fn coach_voice_overlays_and_neutral_stays_plain() {
    // Overridden keys: the tactical alert leads differ by voice, and the
    // coach phrasing never leaks into neutral prose.
    let record = tactical_record();
    let coach = verbalize_voiced(&record, Voice::Coach);
    let neutral = verbalize_voiced(&record, Voice::Neutral);
    assert_ne!(coach, neutral);
    // A phrase only the coach lead uses. "overloaded"/"comes out ahead"
    // would not do: both appear in the shared follow-on sentences, which
    // are voice-independent and must stay that way.
    assert!(
        coach.contains("They can just take"),
        "coach alt lead: {coach}"
    );
    assert!(
        !neutral.contains("They can just take"),
        "coach leak: {neutral}"
    );
    // Both voices ground the same facts: same squares mentioned.
    assert_eq!(squares_in(&coach), squares_in(&neutral));
    // The default voice IS Coach.
    assert_eq!(verbalize(&record), coach);
    assert_eq!(
        verbalize_sections(&record),
        verbalize_sections_voiced(&record, Voice::Coach)
    );

    // A record whose rendering touches no overridden key (engine verdict
    // only) reads identically in both voices: the overlay falls back.
    let mut engine_only = quiet_record();
    engine_only.imbalances.clear();
    engine_only.engine = Some(kibitz_core::record::EngineEval {
        eval_cp: 25,
        mate_in: None,
        best: "Nf3".into(),
        multipv: vec![],
    });
    let c = verbalize_sections_voiced(&engine_only, Voice::Coach);
    let n = verbalize_sections_voiced(&engine_only, Voice::Neutral);
    assert_eq!(c.imbalances, n.imbalances, "fallback must be seamless");

    // Setting tokens round-trip; lenient parse defaults to Coach.
    for voice in Voice::ALL {
        assert_eq!(voice.as_str().parse::<Voice>().unwrap(), voice);
        assert_eq!(Voice::from_setting(voice.as_str()), voice);
    }
    assert_eq!(Voice::from_setting("NEUTRAL"), Voice::Neutral);
    assert_eq!(Voice::from_setting(""), Voice::Coach);
    assert_eq!(Voice::from_setting("garbage"), Voice::Coach);
    assert!("garbage".parse::<Voice>().is_err());
    assert_eq!(Voice::default(), Voice::Coach);
}

/// Run-6: the explanation contract — dual-voice blocks with evidence.
#[test]
fn explanation_contract_dual_voice_with_evidence() {
    use kibitz_core::record::{ArrowKind, BlockKind};
    let r = tactical_record();
    let ex = kibitz_verbalize::explain(&r);
    assert_eq!(ex.schema_version, kibitz_core::record::SCHEMA_VERSION);
    assert_eq!(ex.tag, "TACTICAL SCREEN FIRED");
    // Headline exists in both voices and is not repeated inside the lead
    // block.
    assert!(!ex.headline.coach.is_empty() && !ex.headline.neutral.is_empty());
    let lead = &ex.blocks[0];
    assert!(!lead.text.coach.starts_with(&ex.headline.coach));
    // The lead alert block carries ring + attacker/defender squares and
    // attacker→target arrows.
    assert_eq!(lead.kind, BlockKind::Alert);
    assert_eq!(lead.evidence.alerts, vec!["c6".to_string()]);
    assert!(!lead.evidence.attackers.is_empty());
    assert!(lead
        .evidence
        .arrows
        .iter()
        .all(|a| a.to == "c6" && a.kind == ArrowKind::Attacker));
    assert_eq!(lead.evidence.arrows.len(), lead.evidence.attackers.len());
    // Voices differ in wording, never in evidence (same struct per block).
    assert_ne!(lead.text.coach, lead.text.neutral);
    // Eval readout: confirmed delta for the beneficiary of a BLACK-owned
    // alert converts to White-positive POV.
    let eval = ex.eval.expect("confirmed check yields a readout");
    assert!(eval.cp.unwrap() > 0, "{eval:?}");
    assert!(eval.display.starts_with('+'));
}

#[test]
fn explanation_quiet_position_and_mate_tag() {
    use kibitz_core::record::{EngineEval, EvalReadout};
    let mut r = quiet_record();
    let ex = kibitz_verbalize::explain(&r);
    assert_eq!(ex.tag, "QUIET POSITION");
    assert!(ex
        .blocks
        .iter()
        .all(|b| !b.evidence.alerts.iter().any(String::is_empty)));

    r.engine = Some(EngineEval {
        eval_cp: 31900,
        mate_in: Some(-4),
        best: "Qh2#".into(),
        multipv: vec![],
    });
    let ex = kibitz_verbalize::explain(&r);
    assert_eq!(ex.tag, "FORCED MATE");
    let EvalReadout { mate, display, .. } = ex.eval.unwrap();
    assert_eq!(mate, Some(-4));
    assert_eq!(display, "#4");
}

/// Run 10: candidate-move suggestions ride the Explanation contract,
/// carry the move as a key arrow, and are gated by confirmed tactics,
/// mates and decisive engine lines (tactics outrank plans).
#[test]
fn explanation_carries_gated_suggestions() {
    use kibitz_core::record::{ArrowKind, EngineEval};
    let q = quiet_record();
    let ex = kibitz_verbalize::explain(&q);
    assert!(!ex.suggestions.is_empty(), "quiet record must suggest");
    let top = &ex.suggestions[0];
    assert!(!top.san.is_empty() && top.uci.len() >= 4);
    assert_eq!(top.evidence.arrows.len(), 1);
    assert_eq!(top.evidence.arrows[0].kind, ArrowKind::Key);
    // Here the top pick denies Black's knight plan (e4).
    assert!(top.prophylactic, "{top:?}");
    let js = serde_json::to_value(&ex).unwrap();
    assert!(js["suggestions"][0]["prophylactic"].is_boolean());

    // A confirmed tactic (any size) gates suggestions entirely.
    let ex = kibitz_verbalize::explain(&tactical_record());
    assert!(ex.suggestions.is_empty(), "{:?}", ex.suggestions);

    // A known mate gates them too.
    let mut m = quiet_record();
    m.engine = Some(EngineEval {
        eval_cp: 31_900,
        mate_in: Some(4),
        best: "Qh7#".into(),
        multipv: vec![],
    });
    assert!(kibitz_verbalize::explain(&m).suggestions.is_empty());
}

/// Run-6 residual: at DECISIVE_CP the prose stops counting pawns.
#[test]
fn decisive_band_boundaries() {
    use kibitz_verbalize::DECISIVE_CP;
    let mk = |delta: i32| {
        let mut r = tactical_record();
        if let Some(a) = r.wsui.alerts.first_mut() {
            if let Some(c) = a.engine_check.as_mut() {
                c.score_delta_cp = Some(delta);
                c.mate_in = None;
            }
        }
        verbalize(&r)
    };
    let below = mk(DECISIVE_CP - 1);
    assert!(below.contains("winning about"), "{below}");
    assert!(below.contains("4.99") || below.contains("5"), "{below}");
    let at = mk(DECISIVE_CP);
    assert!(at.contains("simply winning"), "{at}");
    assert!(
        !at.contains("winning about"),
        "engine prose stops counting pawns at the band: {at}"
    );
    let way_above = mk(910);
    assert!(way_above.contains("simply winning"), "{way_above}");
    // Mate still outranks any band.
    let mut r = tactical_record();
    if let Some(a) = r.wsui.alerts.first_mut() {
        if let Some(c) = a.engine_check.as_mut() {
            c.score_delta_cp = Some(10_000);
            c.mate_in = Some(3);
        }
    }
    let mate = verbalize(&r);
    assert!(mate.contains("forced mate in 3"), "{mate}");

    // Whole-position eval band.
    use kibitz_core::record::EngineEval;
    let mut q = quiet_record();
    q.engine = Some(EngineEval {
        eval_cp: 910,
        mate_in: None,
        best: "g4".into(),
        multipv: vec![],
    });
    let out = verbalize(&q);
    assert!(out.contains("completely winning for White"), "{out}");
    assert!(!out.contains("+9.1"), "number stays out of prose: {out}");
}

/// Run-8 maintainer ruling: a confirmed tactic mutes positional prose in
/// proportion to its size; unconfirmed alerts change nothing.
#[test]
fn confirmed_tactic_gates_positional_prose() {
    use kibitz_core::record::{CompositePlan, EngineCheckStatus, Favors, ImbalanceKind};
    // Start from the full record: tactical alert + imbalances + composite.
    let mk = |status: EngineCheckStatus, delta: Option<i32>, mate: Option<i32>| {
        let mut r = tactical_record();
        // Graft the quiet record's positional content onto it.
        let q = quiet_record();
        r.imbalances = q.imbalances.clone();
        r.composite_plans = vec![CompositePlan {
            target: "d5".into(),
            hints: vec![
                "ManeuverKnightToOutpost".into(),
                "PressureBackwardPawn".into(),
            ],
            supporting: vec![ImbalanceKind::SquaresOutposts, ImbalanceKind::PawnStructure],
            squares: vec!["d5".into(), "d6".into()],
            score: 4,
            favors: Favors::White,
        }];
        if let Some(a) = r.wsui.alerts.first_mut() {
            if let Some(c) = a.engine_check.as_mut() {
                c.status = status;
                c.score_delta_cp = delta;
                c.mate_in = mate;
            }
        }
        r
    };

    // Confirmed big swing (or mate): tactics only — no positional prose.
    for r in [
        mk(EngineCheckStatus::Confirmed, Some(250), None),
        mk(EngineCheckStatus::Confirmed, None, Some(4)),
    ] {
        let sections = kibitz_verbalize::verbalize_sections(&r);
        assert!(!sections.tactics.is_empty());
        assert!(
            sections.imbalances.is_empty(),
            "imbalances muted by a dominant confirmed tactic: {}",
            sections.imbalances
        );
        assert!(sections.plans.is_empty(), "plans muted: {}", sections.plans);
    }

    // Confirmed small swing: tactics lead, ONE imbalance survives, no plans.
    let small = mk(EngineCheckStatus::Confirmed, Some(120), None);
    let sections = kibitz_verbalize::verbalize_sections(&small);
    assert!(!sections.imbalances.is_empty(), "top imbalance kept");
    let sentence_count = sections.imbalances.matches(". ").count() + 1;
    assert!(
        sentence_count <= 3,
        "only the top theme: {}",
        sections.imbalances
    );
    assert!(sections.plans.is_empty(), "{}", sections.plans);

    // Unconfirmed: everything renders as before (the screen may be refuted).
    let unclear = mk(EngineCheckStatus::UnclearAtBudget, Some(250), None);
    let sections = kibitz_verbalize::verbalize_sections(&unclear);
    assert!(!sections.imbalances.is_empty());
    assert!(!sections.plans.is_empty());

    // The explanation contract applies the same gate.
    let ex = kibitz_verbalize::explain(&mk(EngineCheckStatus::Confirmed, Some(250), None));
    use kibitz_core::record::BlockKind;
    assert!(
        ex.blocks.iter().all(|b| b.kind == BlockKind::Alert),
        "alert blocks only"
    );
}
