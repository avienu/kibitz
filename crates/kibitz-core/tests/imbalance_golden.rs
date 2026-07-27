//! Golden-file tests for the eight imbalance detectors. Each position
//! cites its source; constructed positions are labeled and model a named
//! instructional pattern.

use cozy_chess::Board;
use kibitz_core::imbalance;
use kibitz_core::record::{Favors, Magnitude, Phase};

fn board(fen: &str) -> Board {
    fen.parse().unwrap()
}

/// Sveshnikov Sicilian tabiya after 6...d6 (1.e4 c5 2.Nf3 Nc6 3.d4 cxd4
/// 4.Nxd4 Nf6 5.Nc3 e5 6.Ndb5 d6) — THE canonical d5-hole + backward-d6
/// structure, discussed in every strategy text (incl. Jeremy Silman's coverage
/// of weak squares). Source: standard opening theory.
#[test]
fn sveshnikov_d5_hole_and_backward_d6() {
    let b = board("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7");
    let sq = imbalance::squares_outposts(&b).expect("squares imbalance");
    assert_eq!(sq.favors, Favors::White);
    let holes = sq.evidence.get("holes_in_black_camp").unwrap().to_string();
    assert!(holes.contains("d5"), "d5 must be a hole: {holes}");

    let ps = imbalance::pawn_structure(&b).expect("pawn imbalance");
    assert_eq!(ps.favors, Favors::White);
    let backward = ps.evidence.get("backward_black").unwrap().to_string();
    assert!(backward.contains("d6"), "d6 is backward: {backward}");
}

/// French Advance structure after 1.e4 e6 2.d4 d5 3.e5 — Black's c8
/// bishop buried behind its own e6/d5 light-square pawns: the canonical
/// "bad French bishop" (every French Defense primer). Source: standard
/// opening theory.
#[test]
fn french_advance_bad_bishop() {
    let b = board("rnbqkbnr/ppp2ppp/4p3/3pP3/3P4/8/PPP2PPP/RNBQKBNR b KQkq - 0 3");
    let mp = imbalance::minor_pieces(&b).expect("minor-piece imbalance");
    assert!(
        mp.evidence.contains_key("bad_bishop_black"),
        "black light-squared bishop is bad: {:?}",
        mp.evidence
    );
    assert_eq!(mp.favors, Favors::White);
}

/// Constructed: protected passed pawn on d5 vs none — passed-pawn
/// pattern (cf. Nimzowitsch, My System, on the passed pawn's "lust to
/// expand").
#[test]
fn protected_passer_scores_for_white() {
    let b = board("4k3/5p2/8/2PP4/8/8/8/4K3 w - - 0 1");
    let ps = imbalance::pawn_structure(&b).expect("pawn imbalance");
    assert_eq!(ps.favors, Favors::White);
    assert!(ps.magnitude >= Magnitude::Clear);
    let passed = ps.evidence.get("passed_white").unwrap().to_string();
    assert!(passed.contains("d5") && passed.contains("c5"), "{passed}");
    // Blockade hint points at the stop square d6.
    assert!(ps
        .plans
        .iter()
        .any(|p| p.hint == "BlockadeWhitePasser" && p.squares.contains(&"d6".to_string())));
}

/// Constructed: rook on an open file + rook on the 7th vs passive rooks —
/// the classic major-piece file battery themes (cf. any rook-endgame
/// primer; "a rook on the seventh").
#[test]
fn open_file_and_seventh_rank() {
    let b = board("2r3k1/R4ppp/8/8/8/8/5PPP/4R1K1 w - - 0 1");
    let fd = imbalance::files_diagonals(&b).expect("files imbalance");
    assert_eq!(fd.favors, Favors::White);
    assert!(fd.evidence.contains_key("rook_on_seventh"));
    assert_eq!(
        fd.evidence.get("rook_on_seventh").unwrap(),
        &serde_json::json!("white")
    );
}

/// Constructed: exchange up (R vs N), material pattern named.
#[test]
fn material_exchange_up() {
    let b = board("4k3/8/8/8/8/2n5/8/3RK3 w - - 0 1");
    let m = imbalance::material(&b).expect("material imbalance");
    assert_eq!(m.favors, Favors::White);
    assert_eq!(
        m.evidence.get("pattern").unwrap(),
        &serde_json::json!("white-exchange-up")
    );
}

/// Constructed: massive central space (Maroczy-bind-like c4+e4 vs d6) —
/// space imbalance pattern (cf. the bind structures in strategy manuals).
#[test]
fn space_advantage_white() {
    let b = board("r1bqkb1r/pp2pppp/2np1n2/8/2PNP3/2N5/PP3PPP/R1BQKB1R b KQkq - 0 6");
    let s = imbalance::space(&b).expect("space imbalance");
    assert_eq!(s.favors, Favors::White);
    assert!(s.plans.iter().any(|p| p.hint == "UseSpaceAvoidExchanges"));
}

/// Constructed: White fully developed and castled, Black untouched —
/// development imbalance (cf. Morphy's opera-game lesson).
#[test]
fn development_lead_white() {
    let b = board("rnbqkbnr/ppp2ppp/3p4/4p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 b kq - 0 4");
    let d = imbalance::development(&b).expect("development imbalance");
    assert_eq!(d.favors, Favors::White);
    assert!(d
        .plans
        .iter()
        .any(|p| p.hint == "OpenPositionBeforeOpponentCompletes"));
}

/// Development is NOT reported deep into the game (spec: move threshold).
#[test]
fn development_silent_after_move_threshold() {
    let b = board("r4rk1/1pp1qppp/p1np1n2/2b1p1b1/2B1P1B1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 30");
    assert!(imbalance::development(&b).is_none());
}

/// Phase classification: material + move based (spec).
#[test]
fn phase_classification() {
    assert_eq!(
        imbalance::phase(&board(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        )),
        Phase::Opening
    );
    assert_eq!(
        imbalance::phase(&board(
            "r4rk1/1pp1qppp/p1np1n2/2b1p1b1/2B1P1B1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 20"
        )),
        Phase::Middlegame
    );
    assert_eq!(
        imbalance::phase(&board("8/3k4/8/3P4/2K5/8/8/8 w - - 0 50")),
        Phase::Endgame
    );
}

/// Composite-plan synthesis (run-5): on the Sveshnikov tabiya the d5
/// complex must merge into ONE plan — knight maneuver to the d5 hole plus
/// pressure on the backward d6 pawn — supported by two independent
/// imbalances. Source: Sveshnikov Sicilian after 6...d6, the canonical
/// "everything points to d5" position in strategy literature.
#[test]
fn sveshnikov_composite_plan_converges_on_d5() {
    let b = board("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7");
    let record = kibitz_core::analyze(&b);
    let top = record.composite_plans.first().expect("composite plan");
    assert_eq!(top.target, "d5");
    assert!(top.supporting.len() >= 2, "{top:?}");
    assert!(top.hints.contains(&"ManeuverKnightToOutpost".to_string()));
    assert!(top.hints.contains(&"PressureBackwardPawn".to_string()));
    assert_eq!(top.favors, Favors::White);
}

/// The full analyze() record on the Sveshnikov tabiya is JSON-stable.
#[test]
fn analyze_snapshot_sveshnikov() {
    let b = board("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7");
    let record = kibitz_core::analyze(&b);
    assert_eq!(record.schema_version, 3);
    insta::assert_json_snapshot!(record, {
        ".provenance.version" => "[version]",
    });
}
