//! Book-trial golden set (run 8.5): the highest-confidence, most
//! diagnostic positions from Jeremy Silman's middlegame books, promoted
//! from the private validation corpus into committed tests. Each entry is
//! FEN + citation + tag assertions only — no book prose. Counter-example
//! positions assert NON-firing: they anchor precision while the
//! detectors chase recall.

use cozy_chess::Board;
use kibitz_core::record::{Favors, ImbalanceKind, Magnitude};

fn analyze(fen: &str) -> kibitz_core::record::FeatureRecord {
    let board: Board = fen.parse().expect("valid FEN");
    kibitz_core::analyze(&board)
}

fn kinds(r: &kibitz_core::record::FeatureRecord) -> Vec<ImbalanceKind> {
    r.imbalances.iter().map(|i| i.kind).collect()
}

fn hints(r: &kibitz_core::record::FeatureRecord) -> Vec<String> {
    r.imbalances
        .iter()
        .flat_map(|i| i.plans.iter().map(|p| p.hint.clone()))
        .collect()
}

fn favors_of(r: &kibitz_core::record::FeatureRecord, kind: ImbalanceKind) -> Option<Favors> {
    r.imbalances
        .iter()
        .find(|i| i.kind == kind)
        .map(|i| i.favors)
}

// --- The Amateur's Mind discriminating pair: identical wing storms,
// --- differing ONLY in whether the center is truly locked. ---

/// Jeremy Silman, The Amateur's Mind, p. 323, test 15.
#[test]
fn amateurs_mind_test_15_storm_justified() {
    let r = analyze("r1bq1rk1/pp1nb1pp/4p3/2ppPp2/5B2/2PBP3/PP1N1PPP/R2QK2R w KQ - 0 1");
    assert!(hints(&r).contains(&"WingPawnStormClosedCenter".to_string()));
}

/// Jeremy Silman, The Amateur's Mind, p. 322, test 14.
#[test]
fn amateurs_mind_test_14_storm_refuted() {
    let r = analyze("r3nrk1/pppq1pbp/2np2p1/4p3/4P3/2NP1NP1/PPP2PKP/R1BQ1R2 w - - 0 1");
    assert!(!hints(&r).contains(&"WingPawnStormClosedCenter".to_string()));
}

// --- Minority attack, both colors. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 202, entry
/// 'Minority Attack'.
#[test]
fn minority_attack_white() {
    let r = analyze("r1bqrnk1/pp2bppp/2p2n2/3p2B1/3P4/2NBPN2/PPQ2PPP/R4RK1 w - - 0 1");
    assert!(hints(&r).contains(&"MinorityAttack".to_string()));
    assert!(kinds(&r).contains(&ImbalanceKind::PawnStructure));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 203, entry
/// 'Minority Attack'.
#[test]
fn minority_attack_black() {
    let r = analyze("r2qkbnr/pp3ppp/2n1p3/3p4/3P4/2PQ1N2/PP3PPP/RNB1K2R w KQkq - 0 1");
    assert!(hints(&r).contains(&"MinorityAttack".to_string()));
}

// --- Outposts and support points. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 272, entry
/// 'Squares'.
#[test]
fn weak_square_d5_owned_and_reachable() {
    let r = analyze("2qrr1k1/pp3bbp/2n2pp1/4p3/2P5/1PN1P1P1/PB1Q1PBP/3RR1K1 w - - 0 1");
    assert!(kinds(&r).contains(&ImbalanceKind::SquaresOutposts));
    assert!(hints(&r).contains(&"ManeuverKnightToOutpost".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 276, entry
/// 'Support Points'.
#[test]
fn established_knight_support_point_c6() {
    let r = analyze("r4rk1/1qb2ppp/1pN2n2/pP2p3/P3P3/3PB3/3RQ1PP/3R2K1 w - - 0 1");
    let so = r
        .imbalances
        .iter()
        .find(|i| i.kind == ImbalanceKind::SquaresOutposts)
        .expect("squares imbalance");
    assert_eq!(
        so.evidence.get("established_outpost_white"),
        Some(&serde_json::json!("c6"))
    );
    assert_eq!(so.favors, Favors::White);
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 277, entry
/// 'Support Points' — bishops hold support points too.
#[test]
fn established_bishop_support_point_d6() {
    let r = analyze("r2r2k1/pp3ppp/2qBp1n1/2P1P3/P7/4Q1PP/1R3PK1/1R6 w - - 0 1");
    let so = r
        .imbalances
        .iter()
        .find(|i| i.kind == ImbalanceKind::SquaresOutposts)
        .expect("squares imbalance");
    assert_eq!(
        so.evidence.get("bishop_outpost_white"),
        Some(&serde_json::json!("d6"))
    );
}

/// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 95, chapter
/// example (minor pieces): the knight aims at the d5 hole even while it
/// is still piece-covered — the defenders are tradeable.
#[test]
fn knight_route_to_contested_hole_d5() {
    let r = analyze("r4rk1/ppq1bppp/3pbn2/4p3/4PP2/2N1B3/PPP1B1PP/R3QRK1 w - - 0 1");
    let so = favors_of(&r, ImbalanceKind::SquaresOutposts);
    assert_eq!(so, Some(Favors::White));
    assert!(hints(&r).contains(&"ManeuverKnightToOutpost".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 219, entry
/// 'Two Knights' — COUNTER-example: fourth-rank posts with no permanent
/// support are not outposts, and no maneuver plan may fire.
#[test]
fn kickable_knights_are_not_outposts() {
    let r = analyze("3rr2k/p1q1b1pp/1p2bp2/2p5/P1N1N3/3P4/1PP1QPPP/R4RK1 b - - 0 1");
    assert!(!hints(&r).contains(&"ManeuverKnightToOutpost".to_string()));
}

// --- Pawn weaknesses: pressure plans and their precision anchors. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 236, entry
/// 'Pawn Structure - Backward Pawns'.
#[test]
fn backward_pawn_pressured_on_half_open_file() {
    let r = analyze("r1q2rk1/1p2bppp/pBnp4/4p3/P7/2NB1QP1/1PP2P1P/R2R2K1 w - - 0 1");
    assert!(hints(&r).contains(&"PressureBackwardPawn".to_string()));
    assert!(kinds(&r).contains(&ImbalanceKind::PawnStructure));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 237, entry
/// 'Pawn Structure - Backward Pawns' — COUNTER-example: the well-defended
/// backward pawn earns no pressure plan.
#[test]
fn well_defended_backward_pawn_not_pressured() {
    let r = analyze("r2r1bk1/1pq1ppp1/p1npbn1p/8/4P3/1NN1BP2/PPPQB1PP/R2R2K1 w - - 0 1");
    assert!(kinds(&r).contains(&ImbalanceKind::PawnStructure));
    assert!(!hints(&r).contains(&"PressureBackwardPawn".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 240, entry
/// 'Pawn Structure - Doubled Pawns'.
#[test]
fn indefensible_front_doubled_pawn_pressured() {
    let r = analyze("rnbq1rk1/p1pp1ppp/1p2pn2/8/2PPP3/P1P2P2/6PP/R1BQKBNR b KQ - 0 1");
    assert!(hints(&r).contains(&"PressureDoubledPawn".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 239, entry
/// 'Pawn Structure - Doubled Pawns' — COUNTER-example: useful doubled
/// pawns are reported as structure but never as a target.
#[test]
fn useful_doubled_pawns_not_a_target() {
    let r = analyze("r1bq1rk1/ppp2pp1/2np1n1p/4p3/2B1P3/2NPPN2/PPP3PP/R2Q1RK1 w - - 0 1");
    assert!(kinds(&r).contains(&ImbalanceKind::PawnStructure));
    assert!(kinds(&r).contains(&ImbalanceKind::FilesDiagonals));
    assert!(!hints(&r).contains(&"PressureDoubledPawn".to_string()));
}

// --- Bad bishop. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 279, entry
/// 'Trading Pieces'.
#[test]
fn bad_bishop_wants_trade_or_freedom() {
    let r = analyze("rnbq1rk1/pp2bpnp/3p2pB/2pPp3/2P1P1P1/2N2N1P/PP1Q1P2/R3R1K1 b - - 0 1");
    assert!(hints(&r).contains(&"TradeOrActivateBadBishop".to_string()));
    assert!(kinds(&r).contains(&ImbalanceKind::MinorPieces));
}

// --- Entombed pieces: an imbalance, not a tactical alert (run 12, #12). ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 192, entry
/// 'Entombed Pieces': Black is a rook up on the ledger and worse on the
/// board, because the b8-rook is buried behind White's b7-pawn forever.
/// The whole claim is that the Material verdict flips.
#[test]
fn entombed_rook_flips_the_material_verdict() {
    let fen = "1rB5/1P6/p4k2/2p5/2P2KP1/8/8/8 w - - 0 1";
    let r = analyze(fen);
    assert!(hints(&r).contains(&"KeepPieceEntombed".to_string()));
    assert_eq!(favors_of(&r, ImbalanceKind::Material), Some(Favors::White));
    // The screen must stay out of it: entombment is strategic, and a
    // trapped-piece alert here would buy an engine job for nothing.
    assert!(r
        .wsui
        .alerts
        .iter()
        .all(|a| a.kind != kibitz_core::record::AlertKind::TrappedPiece));
}

/// Jeremy Silman, The Amateur's Mind, p. 10 (Bishops vs Knights): the
/// f1-bishop is sealed behind five frozen white pawns while Black's
/// knight owns the board.
#[test]
fn entombed_bishop_beside_the_bad_bishop() {
    let r = analyze("8/8/8/6p1/3n1pP1/2pPpP2/k1P1P3/3K1B2 w - - 0 1");
    assert!(hints(&r).contains(&"ActivateEntombedPiece".to_string()));
    assert_eq!(
        favors_of(&r, ImbalanceKind::MinorPieces),
        Some(Favors::Black)
    );
}

/// The precision anchor for the concept: the starting position has eight
/// pieces with nothing to do and not one of them is entombed. What
/// separates them is a single pawn move, which is the entire distinction
/// between an undeveloped piece and a buried one.
#[test]
fn the_opening_position_entombs_nobody() {
    let r = analyze("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    assert!(!hints(&r).contains(&"ActivateEntombedPiece".to_string()));
    assert!(!hints(&r).contains(&"KeepPieceEntombed".to_string()));
}

// --- Files, the seventh rank, and rooks. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 329, entry
/// 'Two Hogs on the Seventh': doubled seventh-rank rooks outweigh the
/// pawn deficit — a winning file imbalance for White while Material
/// still reads Black.
#[test]
fn two_hogs_on_the_seventh() {
    let r = analyze("r3k3/pRR5/8/5p2/6p1/6P1/r4PK1/8 w - - 0 1");
    assert!(hints(&r).contains(&"RookToSeventh".to_string()));
    let fd = r
        .imbalances
        .iter()
        .find(|i| i.kind == ImbalanceKind::FilesDiagonals)
        .expect("files imbalance");
    assert_eq!(fd.favors, Favors::White);
    assert_eq!(fd.magnitude, Magnitude::Winning);
    assert_eq!(favors_of(&r, ImbalanceKind::Material), Some(Favors::Black));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 225, entry
/// 'No Entrance!' — COUNTER-example: an open file whose entry squares
/// are all covered yields no seventh-rank plan.
#[test]
fn open_file_with_no_entry_squares() {
    let r = analyze("r5k1/rbqn1pb1/3p1npp/2pPp3/1pP1P3/1P4NP/1B1Q1PPN/1B2RRK1 b - - 0 1");
    assert!(kinds(&r).contains(&ImbalanceKind::FilesDiagonals));
    assert!(!hints(&r).contains(&"RookToSeventh".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 323, entry
/// 'Rooks Behind Passed Pawns'.
#[test]
fn rook_belongs_behind_the_passer() {
    let r = analyze("8/5pk1/6p1/7p/P7/6P1/2r2PKP/1R6 w - - 0 1");
    assert!(hints(&r).contains(&"RookBehindPasser".to_string()));
}

// --- Blockades. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 178, entry
/// 'Blockade': the knight blockades the d5 passer and pressures the
/// pawns that defend it.
#[test]
fn knight_blockade_then_pressure() {
    let r = analyze("r4rk1/p3qp1p/1p1n2p1/2pPp3/2P1P3/3B4/PP4PP/R2Q1RK1 b - - 0 1");
    let h = hints(&r);
    assert!(h.contains(&"BlockadeWhitePasser".to_string()));
    assert!(h.contains(&"BlockadeThenPressure".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 244, entry
/// 'Pawn Structure - Isolated Pawns': blockade the isolated pawn, then
/// pile up on it.
#[test]
fn isolated_pawn_blockade_formula() {
    let r = analyze("3r2k1/1p3pp1/p1q4p/3p4/3R4/1P2P3/P2Q1PPP/6K1 w - - 0 1");
    assert!(hints(&r).contains(&"BlockadeThenPressure".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 298, entry
/// 'Passed Pawns in a Queen Endgame' — COUNTER-example: the far-advanced
/// a6 passer is the story (blockade IT); Black's unadvanced mass earns
/// no blockade plan, and Material honestly reads Black.
#[test]
fn far_passer_outweighs_unadvanced_majority() {
    let r = analyze("q7/5pk1/P3p1pp/1Q6/8/8/8/6K1 w - - 0 1");
    let h = hints(&r);
    assert!(h.contains(&"BlockadeWhitePasser".to_string()));
    assert!(!h.contains(&"BlockadeBlackPasser".to_string()));
    assert_eq!(favors_of(&r, ImbalanceKind::Material), Some(Favors::Black));
}

// --- Majorities. ---

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 269, entry
/// 'Queenside Pawn Majority' — COUNTER-example: in a middlegame the
/// central majority is the plan; the queenside hint is withheld.
#[test]
fn central_majority_beats_queenside_majority() {
    let r = analyze("2rr2k1/p1qn1ppb/1p2p2p/8/2P5/1P2BN2/P3QPPP/3RR1K1 b - - 0 1");
    let h = hints(&r);
    assert!(h.contains(&"AdvanceCentralMajority".to_string()));
    assert!(!h.contains(&"AdvanceQueensideMajority".to_string()));
}

/// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 367, problem
/// 27: queens off — push the queenside majority and use the king.
#[test]
fn queenside_majority_with_active_king() {
    let r = analyze("r4rk1/pb1p1ppp/1p2pn2/8/1PPP4/P1N2P2/3K2PP/R4B1R b - - 0 1");
    let h = hints(&r);
    assert!(h.contains(&"AdvanceQueensideMajority".to_string()));
    assert!(h.contains(&"ActivateKingInEndgame".to_string()));
}

// --- Minor-piece stories. ---

/// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 371, problem
/// 82: the stranded knight has no home; deny it one and open the game
/// for the bishop.
#[test]
fn restrict_the_homeless_knight() {
    let r = analyze("1n1rr1k1/p1p2ppp/1p1p4/4q3/2P5/P3PB2/1PQR1PPP/5RK1 w - - 0 1");
    let h = hints(&r);
    assert!(h.contains(&"RestrictKnight".to_string()));
    assert!(h.contains(&"OpenPositionForBishops".to_string()));
}

/// Jeremy Silman, The Complete Book of Chess Strategy, p. 296, entry
/// 'Minor Pieces in the Endgame': opposite-colored bishops are a named
/// imbalance even in a level position.
#[test]
fn opposite_colored_bishops_reported() {
    let r = analyze("6k1/5p1p/4b1p1/1p6/pP5P/P5P1/1B3P2/6K1 w - - 0 1");
    let mp = r
        .imbalances
        .iter()
        .find(|i| i.kind == ImbalanceKind::MinorPieces)
        .expect("minor-piece imbalance");
    assert_eq!(
        mp.evidence.get("opposite_bishops"),
        Some(&serde_json::json!(true))
    );
}

// --- Attack membrane. ---

/// Jeremy Silman, The Amateur's Mind, p. 316, test 2: an airy king and a
/// usable half-open file beside it.
#[test]
fn open_lines_toward_the_weak_king() {
    let r = analyze("r3r1k1/pb3p2/4pR2/1p1p2p1/3P1n2/B1P5/PP1N2PP/R5K1 w - - 0 1");
    assert!(hints(&r).contains(&"OpenLinesTowardWeakKing".to_string()));
    assert_eq!(
        favors_of(&r, ImbalanceKind::FilesDiagonals),
        Some(Favors::White)
    );
}

/// Jeremy Silman, The Amateur's Mind, p. 2, chapter example
/// (Imbalances), Dzindzichashvili-Yermolinsky, U.S. Championship 1993:
/// the book's own full imbalance census — the record must name the
/// minor-piece, structure, file and development stories at once.
#[test]
fn amateurs_mind_census_position() {
    let r = analyze("rn2k2r/pppbqpb1/4p1pp/4P3/8/2NB1N2/PPP2PPP/R2Q1RK1 w kq - 0 1");
    let k = kinds(&r);
    assert!(k.contains(&ImbalanceKind::MinorPieces));
    assert!(k.contains(&ImbalanceKind::PawnStructure));
    assert!(k.contains(&ImbalanceKind::FilesDiagonals));
    assert!(k.contains(&ImbalanceKind::Development));
    assert_eq!(
        favors_of(&r, ImbalanceKind::Development),
        Some(Favors::White)
    );
}
