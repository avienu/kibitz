//! Bounded engine verification of static move suggestions (run 11).
//!
//! The maintainer's ruling on the Winawer field report ("f5?? shipped as
//! a chip"): "we need to include at least a cursory engine review, at
//! least if tactics screen is present (WSUI)". A fired WSUI screen is the
//! sanctioned engine trigger (CLAUDE.md #6) — when it fires, the same
//! bounded engine that grades the alert also reviews the static
//! candidate moves: a baseline search of the position plus one cursory
//! `go nodes` search per candidate (≤3 candidates + baseline ≤ 4
//! searches).
//!
//! The DECISION is a pure function here so it is testable without an
//! engine; the searches live with their callers (the wsui-confirm job in
//! [`crate::jobs`], the live `verify_suggestions` IPC in src-tauri).

use serde::Serialize;

/// Node budget for each cursory verification search ("cursory" per the
/// maintainer: the same order as the WSUI confirm budget, 100k–200k).
pub const VERIFY_NODES: u64 = 150_000;

/// A candidate is REFUTED when its eval falls more than this many
/// centipawns below the position's baseline (best-play) eval.
pub const REFUTE_MARGIN_CP: i32 = 150;

/// Mate scores fold into centipawns at this sentinel (matches
/// [`crate::engine`]'s ±10000 convention), so the margin rule refutes a
/// candidate that walks into a mate without a separate code path.
pub const MATE_SENTINEL_CP: i32 = 10_000;

/// One candidate's bounded-search outcome, from the MOVER's point of
/// view (searches of the position AFTER the candidate report the
/// opponent's POV — negate before building this).
#[derive(Debug, Clone)]
pub struct CandidateEval {
    pub uci: String,
    /// The static whole-board veto's mark (kibitz-core::suggest).
    pub static_risk: Option<i32>,
    /// Mover-POV score, mate folded to ±[`MATE_SENTINEL_CP`]; `None`
    /// when the search produced nothing usable.
    pub score: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Cleared,
    Refuted,
}

/// Fold a `(score_cp, mate)` pair into a single mover-POV centipawn
/// score (mate wins over cp, signed by who mates).
pub fn fold_score(score_cp: Option<i32>, mate: Option<i32>) -> Option<i32> {
    match (mate, score_cp) {
        (Some(m), _) => Some(if m > 0 {
            MATE_SENTINEL_CP
        } else {
            -MATE_SENTINEL_CP
        }),
        (None, cp) => cp,
    }
}

/// The pure verification decision (run 11): given the position's
/// baseline eval (side to move's POV) and each candidate's mover-POV
/// eval, a candidate is REFUTED when it falls more than
/// [`REFUTE_MARGIN_CP`] below the baseline — that covers both "clearly
/// worse than the best move" and "loses outright material", since the
/// baseline is what best play keeps. Statically-marked candidates need
/// an eval to be CLEARED (no eval = stays vetoed); statically-clean
/// candidates survive unless the engine refutes them.
pub fn decide(baseline: i32, candidates: &[CandidateEval]) -> Vec<(String, Verdict)> {
    candidates
        .iter()
        .map(|c| {
            let verdict = match c.score {
                Some(score) if baseline - score > REFUTE_MARGIN_CP => Verdict::Refuted,
                Some(_) => Verdict::Cleared,
                None if c.static_risk.is_some() => Verdict::Refuted,
                None => Verdict::Cleared,
            };
            (c.uci.clone(), verdict)
        })
        .collect()
}

/// Convenience: the uci moves [`decide`] cleared, in candidate order.
pub fn cleared_moves(baseline: i32, candidates: &[CandidateEval]) -> Vec<String> {
    decide(baseline, candidates)
        .into_iter()
        .filter_map(|(uci, v)| (v == Verdict::Cleared).then_some(uci))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(uci: &str, static_risk: Option<i32>, score: Option<i32>) -> CandidateEval {
        CandidateEval {
            uci: uci.into(),
            static_risk,
            score,
        }
    }

    /// The Winawer shape (maintainer field report): baseline ≈ equal;
    /// cxd4 (marked, engine says fine) is cleared, f5/f6 (marked, engine
    /// says a piece is gone) are refuted.
    #[test]
    fn winawer_shape_clears_theory_and_refutes_droppers() {
        let out = decide(
            -20,
            &[
                cand("c5d4", Some(230), Some(-35)),
                cand("f7f5", Some(230), Some(-260)),
                cand("f7f6", Some(230), Some(-310)),
            ],
        );
        assert_eq!(out[0], ("c5d4".into(), Verdict::Cleared));
        assert_eq!(out[1], ("f7f5".into(), Verdict::Refuted));
        assert_eq!(out[2], ("f7f6".into(), Verdict::Refuted));
        assert_eq!(
            cleared_moves(-20, &[cand("c5d4", Some(230), Some(-35))]),
            ["c5d4"]
        );
    }

    /// Clean candidates survive unless refuted; the margin is strict.
    #[test]
    fn margin_is_strict_and_clean_survives_missing_evals() {
        // Exactly at the margin: NOT refuted.
        let out = decide(100, &[cand("a", None, Some(-50))]);
        assert_eq!(out[0].1, Verdict::Cleared);
        // One centipawn beyond: refuted, even when statically clean.
        let out = decide(100, &[cand("a", None, Some(-51))]);
        assert_eq!(out[0].1, Verdict::Refuted);
        // No eval: clean survives (it was fine statically)…
        let out = decide(0, &[cand("a", None, None)]);
        assert_eq!(out[0].1, Verdict::Cleared);
        // …but a marked candidate NEEDS the engine to clear it.
        let out = decide(0, &[cand("a", Some(230), None)]);
        assert_eq!(out[0].1, Verdict::Refuted);
    }

    /// A piece already en prise: the baseline is itself bad, so a marked
    /// candidate scoring near the baseline (the best available) clears.
    #[test]
    fn losing_baseline_clears_the_least_bad_marked_candidate() {
        let out = decide(-240, &[cand("save", Some(230), Some(-250))]);
        assert_eq!(out[0].1, Verdict::Cleared);
    }

    /// Mate folding: walking into a mate is refuted via the sentinel;
    /// delivering one clears.
    #[test]
    fn mate_folds_into_the_margin_rule() {
        assert_eq!(fold_score(Some(120), Some(-3)), Some(-MATE_SENTINEL_CP));
        assert_eq!(fold_score(None, Some(5)), Some(MATE_SENTINEL_CP));
        assert_eq!(fold_score(Some(42), None), Some(42));
        assert_eq!(fold_score(None, None), None);
        let out = decide(10, &[cand("bad", None, fold_score(None, Some(-2)))]);
        assert_eq!(out[0].1, Verdict::Refuted);
    }
}
