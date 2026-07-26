//! silman-core: static chess feature detectors on top of `cozy-chess`.
//!
//! This crate is BSD-3-Clause and must never depend on GPL code. See CLAUDE.md.

pub mod attack;
pub mod perft;
pub mod record;
pub mod see;
pub mod wsui;

pub use cozy_chess;
