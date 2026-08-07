//! kibitz-core: static chess feature detectors on top of `cozy-chess`.
//!
//! This crate is BSD-3-Clause and must never depend on GPL code. See CLAUDE.md.

pub mod attack;
pub mod development;
pub mod imbalance;
pub mod pawn_contact;
pub mod perft;
pub mod plans;
pub mod prose_gate;
pub mod record;
pub mod route;
pub mod scheme;
pub mod see;
pub mod suggest;
pub mod wsui;

pub use cozy_chess;

/// One-call static analysis: WSUI screen + imbalance assessment + phase.
/// Engine fields stay `None` — filling them is the app layer's job and
/// happens only on fired screens or explicit user request (CLAUDE.md #6).
/// [`analyze`] with the game's move history (run 11): the final position
/// is analyzed as usual, then the development tracker's prior dreams are
/// folded in (opening phase only — see [`development::track`]). Callers
/// that must gate the prior on external state (the openings book) call
/// `development::{track, augment}` themselves.
pub fn analyze_with_history(
    start: &cozy_chess::Board,
    moves: &[cozy_chess::Move],
) -> record::FeatureRecord {
    let mut board = start.clone();
    for &mv in moves {
        board.play(mv);
    }
    let mut record = analyze(&board);
    development::augment(&mut record, &development::track(start, moves));
    record
}

pub fn analyze(board: &cozy_chess::Board) -> record::FeatureRecord {
    let imbalances = imbalance::assess(board);
    let composite_plans = plans::synthesize(&imbalances);
    let maneuvers = route::extract(board, &imbalances);
    let schemes = scheme::synthesize(board, &maneuvers, &composite_plans);
    record::FeatureRecord {
        schema_version: record::SCHEMA_VERSION,
        fen: board.to_string(),
        side_to_move: board.side_to_move().into(),
        phase: imbalance::phase(board),
        wsui: wsui::screen(board, &wsui::WsuiConfig::default()),
        imbalances,
        composite_plans,
        maneuvers,
        schemes,
        engine: None,
        provenance: record::FeatureRecord::provenance_now(),
    }
}
