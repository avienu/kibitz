//! silman-profile: batch corpus analysis producing player profiles.
//!
//! Currently: the repertoire fingerprint (Phase 2). The full PlayerProfile
//! (per-phase ACPL, motif matrix, conversion rates) is Phase 4. This crate
//! is BSD-3-Clause, must never depend on GPL code, and performs no I/O —
//! callers feed it precomputed data. See CLAUDE.md.

pub mod fingerprint;

pub use fingerprint::{
    fingerprint, Color, ColorFingerprint, DeviationPoint, EcoFamilyStat, FingerprintGame,
    FingerprintOptions, GameScore, MoveStat, OwnMove, PositionStat, RepertoireFingerprint,
};
