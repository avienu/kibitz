//! silman-core: static chess feature detectors on top of `cozy-chess`.
//!
//! This crate is BSD-3-Clause and must never depend on GPL code. See CLAUDE.md.

pub mod attack;
pub mod imbalance;
pub mod perft;
pub mod plans;
pub mod record;
pub mod see;
pub mod wsui;

pub use cozy_chess;

/// One-call static analysis: WSUI screen + imbalance assessment + phase.
/// Engine fields stay `None` — filling them is the app layer's job and
/// happens only on fired screens or explicit user request (CLAUDE.md #6).
pub fn analyze(board: &cozy_chess::Board) -> record::FeatureRecord {
    let imbalances = imbalance::assess(board);
    let composite_plans = plans::synthesize(&imbalances);
    record::FeatureRecord {
        schema_version: record::SCHEMA_VERSION,
        fen: board.to_string(),
        side_to_move: board.side_to_move().into(),
        phase: imbalance::phase(board),
        wsui: wsui::screen(board, &wsui::WsuiConfig::default()),
        imbalances,
        composite_plans,
        engine: None,
        provenance: record::FeatureRecord::provenance_now(),
    }
}
