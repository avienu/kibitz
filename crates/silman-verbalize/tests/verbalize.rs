//! Snapshot and property tests for template-mode verbalization.
//!
//! The four hand-built records cover: a tactical position (high-severity
//! InadequatelyDefended with a confirmed engine check, plus a loose rook),
//! a quiet positional middlegame (three imbalances, minor one dropped),
//! an endgame with a passed pawn, and an empty record. The property test
//! proves the hard guarantee: every square mentioned in the prose appears
//! somewhere in the record's own data — the verbalizer invents nothing.

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

/// (b) Quiet positional middlegame: three imbalances; the minor one
/// (Black's e4 outpost) must be dropped by the 3+ rule, and its plan with it.
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
            evidence: BTreeMap::from([
                ("isolated".to_string(), serde_json::json!(["d5"])),
                ("half_open_files".to_string(), serde_json::json!(["d"])),
            ]),
            plans: vec![PlanHint {
                hint: "BlockadeThenPressure".into(),
                squares: vec!["d4".into(), "d5".into()],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::MinorPieces,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence: BTreeMap::from([("bishop_pair".to_string(), serde_json::json!(true))]),
            plans: vec![PlanHint {
                hint: "OpenThePosition".into(),
                squares: vec![],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::SquaresOutposts,
            favors: Favors::Black,
            magnitude: Magnitude::Minor,
            evidence: BTreeMap::from([("outposts".to_string(), serde_json::json!(["e4"]))]),
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
            evidence: BTreeMap::from([("passed".to_string(), serde_json::json!(["a5"]))]),
            plans: vec![PlanHint {
                hint: "EscortThePasser".into(),
                squares: vec!["a5".into(), "a6".into(), "a7".into(), "a8".into()],
            }],
        },
        Imbalance {
            kind: ImbalanceKind::Material,
            favors: Favors::Balanced,
            magnitude: Magnitude::Minor,
            evidence: BTreeMap::new(),
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

#[test]
fn tactical_prose() {
    let out = verbalize(&tactical_record());
    assert_no_leaked_placeholders(&out);
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
    assert_no_leaked_placeholders(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn endgame_prose() {
    let out = verbalize(&endgame_record());
    assert_no_leaked_placeholders(&out);
    insta::assert_snapshot!(out);
}

#[test]
fn empty_record_prose() {
    let out = verbalize(&empty_record());
    assert_no_leaked_placeholders(&out);
    insta::assert_snapshot!(out);
}

fn assert_no_leaked_placeholders(text: &str) {
    assert!(
        !text.contains('{') && !text.contains('}'),
        "template placeholder leaked into output:\n{text}"
    );
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
    for record in [
        tactical_record(),
        quiet_record(),
        endgame_record(),
        empty_record(),
    ] {
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
