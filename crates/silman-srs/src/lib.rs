//! silman-srs: pure FSRS-4.5 spaced-repetition scheduler.
//!
//! Direct implementation of the published FSRS-4.5 algorithm description
//! (<https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm>,
//! FSRS-4.5 + FSRS v4 sections; the parameters and formulas are public).
//! The `fsrs` crate was evaluated and rejected: the crate itself is
//! BSD-3-Clause, but its dependency tree pulls `priority-queue`
//! (LGPL-3.0-or-later OR MPL-2.0), which fails this repository's license
//! gate for BSD crates, plus a full ML optimizer stack this scheduler does
//! not need. See docs/LICENSES.md.
//!
//! Pure logic: no I/O, no network, no clock — callers supply elapsed time
//! in days and interpret the returned interval. Memory state is a versioned
//! serde struct (`MemoryState`); breaking changes bump
//! [`MEMORY_STATE_VERSION`].

use serde::{Deserialize, Serialize};

/// Version of the [`MemoryState`] schema and of the scheduling math.
/// Version 1 = FSRS-4.5 (17 parameters, DECAY −0.5, FACTOR 19/81).
pub const MEMORY_STATE_VERSION: u32 = 1;

/// Number of FSRS-4.5 model parameters.
pub const PARAM_COUNT: usize = 17;

/// Published FSRS-4.5 default parameters w0..w16.
pub const DEFAULT_PARAMS: [f64; PARAM_COUNT] = [
    0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367, 1.0461, 2.1072,
    0.0793, 0.3246, 1.587, 0.2272, 2.8755,
];

/// Forgetting-curve exponent (FSRS-4.5).
const DECAY: f64 = -0.5;
/// Forgetting-curve factor (FSRS-4.5); chosen so R(S, S) = 0.9.
const FACTOR: f64 = 19.0 / 81.0;

const MIN_DIFFICULTY: f64 = 1.0;
const MAX_DIFFICULTY: f64 = 10.0;
const MIN_STABILITY: f64 = 0.01;

/// Review grade, Anki-style. `Again` is a lapse; the rest are successes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    /// The 1..=4 rating used by the FSRS formulas.
    pub fn value(self) -> f64 {
        match self {
            Grade::Again => 1.0,
            Grade::Hard => 2.0,
            Grade::Good => 3.0,
            Grade::Easy => 4.0,
        }
    }
}

/// FSRS memory state of one card: stability (days until retrievability
/// falls to 90%) and difficulty (1..10). Versioned serde contract; see
/// [`MEMORY_STATE_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryState {
    /// Stability S in days; S > 0.
    pub stability: f64,
    /// Difficulty D, clamped to [1, 10].
    pub difficulty: f64,
}

/// One scheduling step: the card's new memory state and the raw (unrounded)
/// next interval in days at the scheduler's desired retention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Review {
    pub memory: MemoryState,
    pub interval_days: f64,
}

/// FSRS-4.5 scheduler. Construct once and reuse; all methods are pure.
#[derive(Debug, Clone)]
pub struct Scheduler {
    params: [f64; PARAM_COUNT],
    desired_retention: f64,
    max_interval_days: f64,
}

impl Default for Scheduler {
    /// Default parameters, 90% desired retention, 100-year interval cap.
    fn default() -> Self {
        Self::new(DEFAULT_PARAMS, 0.9, 36500.0)
    }
}

impl Scheduler {
    /// `desired_retention` must be in (0, 1); `max_interval_days` >= 1.
    pub fn new(params: [f64; PARAM_COUNT], desired_retention: f64, max_interval_days: f64) -> Self {
        assert!(
            desired_retention > 0.0 && desired_retention < 1.0,
            "desired retention must be in (0, 1)"
        );
        assert!(max_interval_days >= 1.0, "max interval must be >= 1 day");
        Self {
            params,
            desired_retention,
            max_interval_days,
        }
    }

    pub fn params(&self) -> &[f64; PARAM_COUNT] {
        &self.params
    }

    pub fn desired_retention(&self) -> f64 {
        self.desired_retention
    }

    /// R(t, S) = (1 + FACTOR · t/S)^DECAY — probability of recall after
    /// `elapsed_days` at stability `stability`. R(S, S) = 0.9 by design.
    pub fn retrievability(&self, stability: f64, elapsed_days: f64) -> f64 {
        let s = stability.max(MIN_STABILITY);
        let t = elapsed_days.max(0.0);
        (1.0 + FACTOR * t / s).powf(DECAY)
    }

    /// I(r, S) = S/FACTOR · (r^(1/DECAY) − 1), clamped to
    /// [1, max_interval] days. I(0.9, S) = S by design.
    pub fn interval_days(&self, stability: f64) -> f64 {
        let raw = stability.max(MIN_STABILITY) / FACTOR
            * (self.desired_retention.powf(1.0 / DECAY) - 1.0);
        raw.clamp(1.0, self.max_interval_days)
    }

    /// Memory state after the very first rating of a new card:
    /// S0(G) = w[G−1], D0(G) = w4 − (G−3)·w5.
    pub fn initial(&self, grade: Grade) -> MemoryState {
        let w = &self.params;
        let g = grade.value();
        MemoryState {
            stability: w[grade as usize].max(MIN_STABILITY),
            difficulty: (w[4] - (g - 3.0) * w[5]).clamp(MIN_DIFFICULTY, MAX_DIFFICULTY),
        }
    }

    /// Memory state after reviewing a card `elapsed_days` after its last
    /// review. Difficulty mean-reverts toward D0(3) with weight w7; new
    /// stability follows the success formula (Hard/Good/Easy, with hard
    /// penalty w15 and easy bonus w16) or the post-lapse formula (Again).
    pub fn review(&self, state: MemoryState, elapsed_days: f64, grade: Grade) -> MemoryState {
        let w = &self.params;
        let g = grade.value();
        let s = state.stability.max(MIN_STABILITY);
        let d = state.difficulty.clamp(MIN_DIFFICULTY, MAX_DIFFICULTY);
        let r = self.retrievability(s, elapsed_days);

        // D'(D, G) = w7 · D0(3) + (1 − w7) · (D − w6·(G − 3)).
        let d0_good = w[4];
        let difficulty = (w[7] * d0_good + (1.0 - w[7]) * (d - w[6] * (g - 3.0)))
            .clamp(MIN_DIFFICULTY, MAX_DIFFICULTY);

        let stability = match grade {
            // S'_f(D, S, R) = w11 · D^−w12 · ((S+1)^w13 − 1) · e^(w14·(1−R)).
            Grade::Again => {
                w[11] * d.powf(-w[12]) * ((s + 1.0).powf(w[13]) - 1.0) * (w[14] * (1.0 - r)).exp()
            }
            // S'_r(D, S, R, G) = S · (e^w8 · (11−D) · S^−w9 ·
            //                        (e^(w10·(1−R)) − 1) · hard · easy + 1).
            _ => {
                let hard = if grade == Grade::Hard { w[15] } else { 1.0 };
                let easy = if grade == Grade::Easy { w[16] } else { 1.0 };
                s * (w[8].exp()
                    * (11.0 - d)
                    * s.powf(-w[9])
                    * ((w[10] * (1.0 - r)).exp() - 1.0)
                    * hard
                    * easy
                    + 1.0)
            }
        };

        MemoryState {
            stability: stability.clamp(MIN_STABILITY, self.max_interval_days),
            difficulty,
        }
    }

    /// One scheduling step: first rating when `state` is `None`, review
    /// otherwise. Returns the new memory state plus the next interval.
    pub fn next(&self, state: Option<MemoryState>, elapsed_days: f64, grade: Grade) -> Review {
        let memory = match state {
            None => self.initial(grade),
            Some(m) => self.review(m, elapsed_days, grade),
        };
        Review {
            memory,
            interval_days: self.interval_days(memory.stability),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    /// Published invariants of the FSRS-4.5 forgetting curve: R(S, S) = 0.9
    /// and I(0.9, S) = S (wiki, FSRS-4.5 section).
    #[test]
    fn forgetting_curve_invariants() {
        let s = Scheduler::default();
        for stab in [3.7145, 42.0, 1000.0] {
            assert!(close(s.retrievability(stab, stab), 0.9), "R(S,S) at {stab}");
            assert!(close(s.interval_days(stab), stab), "I(0.9,S) at {stab}");
        }
        // Sub-day stabilities clamp to the 1-day scheduling minimum.
        assert!(close(s.retrievability(0.5, 0.5), 0.9));
        assert!(close(s.interval_days(0.5), 1.0));
        // Zero elapsed time = certain recall; retention decreases with time.
        assert!(close(s.retrievability(10.0, 0.0), 1.0));
        assert!(s.retrievability(10.0, 5.0) > s.retrievability(10.0, 20.0));
    }

    /// First-rating stabilities are the published defaults w0..w3, so the
    /// first intervals at 90% retention are 0.4872 / 1.4003 / 3.7145 /
    /// 13.8206 days (clamped to >= 1).
    #[test]
    fn parameter_defaults_drive_first_intervals() {
        let s = Scheduler::default();
        assert_eq!(*s.params(), DEFAULT_PARAMS);
        assert!(close(s.initial(Grade::Again).stability, 0.4872));
        assert!(close(s.initial(Grade::Hard).stability, 1.4003));
        assert!(close(s.initial(Grade::Good).stability, 3.7145));
        assert!(close(s.initial(Grade::Easy).stability, 13.8206));
        // D0(G) = w4 − (G−3)·w5: Again hardest, Easy easiest.
        assert!(close(s.initial(Grade::Good).difficulty, 5.1618));
        assert!(close(
            s.initial(Grade::Again).difficulty,
            5.1618 + 2.0 * 1.2298
        ));
        assert!(close(s.initial(Grade::Easy).difficulty, 5.1618 - 1.2298));
        // Rounded first intervals: Again same-day (clamped to 1), Good 4d.
        assert!(close(s.next(None, 0.0, Grade::Again).interval_days, 1.0));
        assert!(close(s.next(None, 0.0, Grade::Good).interval_days, 3.7145));
    }

    /// Reference sequence computed from the published formulas: repeated
    /// Good reviews exactly at due (elapsed = S, so R = 0.9). Difficulty
    /// starts at D0(3) = w4 and is a fixed point of the mean reversion, so
    /// stability alone drives the growth: 3.7145 → 14.094985 → 46.920397 →
    /// 139.626689 → 377.296453 days.
    #[test]
    fn interval_growth_reference_sequence() {
        let s = Scheduler::default();
        let expected = [3.7145, 14.094985, 46.920397, 139.626689, 377.296453];
        let mut r = s.next(None, 0.0, Grade::Good);
        assert!(close(r.memory.stability, expected[0]));
        for want in &expected[1..] {
            let elapsed = r.memory.stability; // review exactly at due
            r = s.next(Some(r.memory), elapsed, Grade::Good);
            assert!(
                (r.memory.stability - want).abs() < 1e-4,
                "expected {want}, got {}",
                r.memory.stability
            );
            assert!(close(r.memory.difficulty, 5.1618), "D fixed at D0(3)");
            assert!(close(r.interval_days, r.memory.stability));
        }
    }

    /// A lapse collapses stability via the post-lapse formula and raises
    /// difficulty. Reference: Good (S = 14.094985) then Again at due →
    /// S = 3.064799, D = 6.901155 (computed from the published formulas).
    #[test]
    fn lapse_resets_stability_and_raises_difficulty() {
        let s = Scheduler::default();
        let good = s.next(None, 0.0, Grade::Good);
        let after_good = s.next(Some(good.memory), good.memory.stability, Grade::Good);
        let lapsed = s.next(
            Some(after_good.memory),
            after_good.memory.stability,
            Grade::Again,
        );
        assert!((lapsed.memory.stability - 3.064799).abs() < 1e-4);
        assert!((lapsed.memory.difficulty - 6.901155).abs() < 1e-4);
        assert!(lapsed.memory.stability < after_good.memory.stability / 4.0);
        assert!(lapsed.memory.difficulty > after_good.memory.difficulty);
    }

    /// Worked example from the algorithm wiki (post-lapse stability
    /// section): with w11 = 2, w12 = 0.2, w13 = 0.2, w14 = 1, D = 2,
    /// R = 0.9, S'_f(S = 100) ≈ 3.
    #[test]
    fn wiki_post_lapse_worked_example() {
        let mut params = DEFAULT_PARAMS;
        params[11] = 2.0;
        params[12] = 0.2;
        params[13] = 0.2;
        params[14] = 1.0;
        let s = Scheduler::new(params, 0.9, 36500.0);
        let state = MemoryState {
            stability: 100.0,
            difficulty: 2.0,
        };
        // Elapsed = S ⇒ R = 0.9, matching the example's R.
        let out = s.review(state, 100.0, Grade::Again);
        assert!(
            (out.stability - 2.918822).abs() < 1e-4,
            "got {}",
            out.stability
        );
    }

    /// Hard penalty (w15) and Easy bonus (w16) order the outcomes:
    /// Hard < Good < Easy next stability, all >= previous S.
    #[test]
    fn grade_ordering_after_success() {
        let s = Scheduler::default();
        let m = s.initial(Grade::Good);
        let hard = s.review(m, m.stability, Grade::Hard).stability;
        let good = s.review(m, m.stability, Grade::Good).stability;
        let easy = s.review(m, m.stability, Grade::Easy).stability;
        assert!((hard - 6.072946).abs() < 1e-4);
        assert!((good - 14.094985).abs() < 1e-4);
        assert!((easy - 33.563586).abs() < 1e-4);
        assert!(m.stability <= hard && hard < good && good < easy);
    }

    /// Difficulty stays clamped to [1, 10] under extreme grade streaks.
    #[test]
    fn difficulty_clamps() {
        let s = Scheduler::default();
        let mut m = s.initial(Grade::Again);
        for _ in 0..50 {
            m = s.review(m, 0.5, Grade::Again);
        }
        assert!(m.difficulty <= 10.0 && m.difficulty >= 1.0);
        assert!((m.difficulty - 10.0).abs() < 1.0, "streak of Again → hard");
        let mut e = s.initial(Grade::Easy);
        for _ in 0..50 {
            e = s.review(e, e.stability, Grade::Easy);
        }
        assert!(e.difficulty >= 1.0, "streak of Easy stays in range");
    }

    /// MemoryState and Grade are a serde contract (stored in the app's
    /// database): round-trip must be exact, field names stable.
    #[test]
    fn serde_round_trip() {
        let m = MemoryState {
            stability: 14.094985,
            difficulty: 5.1618,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"stability\":") && json.contains("\"difficulty\":"));
        let back: MemoryState = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        for g in [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy] {
            let j = serde_json::to_string(&g).unwrap();
            let b: Grade = serde_json::from_str(&j).unwrap();
            assert_eq!(g, b);
        }
        assert_eq!(serde_json::to_string(&Grade::Again).unwrap(), "\"again\"");
    }
}
