//! Schematic thinking: turn converging plans into ORDERED sequences.
//!
//! [`crate::plans`] answers a spatial question — which hints point at the
//! same square. That is a taxonomy, and a taxonomy cannot express the
//! thing the books actually teach: *first* trade the defenders of d5,
//! *then* land the knight, *then* press the weakness behind it (Jeremy
//! Silman, How to Reassess Your Chess, ex. 60). Nimzowitsch's
//! restrain-blockade-destroy is the same shape — one plan, three stages,
//! and the order is the whole lesson.
//!
//! A scheme is emitted only where there is genuine sequence. A plan with
//! a single stage is already fully described by its `CompositePlan`, and
//! wrapping it in ceremony would just be noise.

use crate::record::{CompositePlan, Favors, Maneuver, Scheme, SchemeStep};

/// Hints that describe cashing in on a square somebody already owns,
/// rather than getting there. These belong AFTER the maneuver.
const EXPLOIT_HINTS: &[&str] = &[
    "PressureBackwardPawn",
    "PressureDoubledPawn",
    "DoubleOnOpenFile",
    "RookToSeventh",
    "BlockadeThenPressure",
    "OpenLinesTowardWeakKing",
];

/// Build the ordered sequences implied by the maneuvers and the composite
/// plans that share their target squares. Longest-horizon plans are not
/// "best" — schemes are ranked by how much converging support they have,
/// then by being cheap to start.
pub fn synthesize(maneuvers: &[Maneuver], composites: &[CompositePlan]) -> Vec<Scheme> {
    // One scheme per square, not per piece. When a knight and a bishop
    // both want d5 they are ALTERNATIVE ways into one plan, and listing
    // the plan twice reads as two ideas when there is only one.
    let mut order: Vec<(String, Favors)> = Vec::new();
    for m in maneuvers {
        let key = (m.to.clone(), m.favors);
        if !order.contains(&key) {
            order.push(key);
        }
    }

    let mut out: Vec<Scheme> = Vec::new();
    for (target, favors) in order {
        let group: Vec<&Maneuver> = maneuvers
            .iter()
            .filter(|m| m.to == target && m.favors == favors)
            .collect();
        let mut steps = Vec::new();

        // Stage 1 — clear the way. `blocked_by` names enemy pieces already
        // covering the destination. A hole is permanent but piece cover is
        // tradeable, so this is a prerequisite, not a refutation.
        let mut blockers: Vec<String> = Vec::new();
        for m in &group {
            for b in &m.blocked_by {
                if !blockers.contains(b) {
                    blockers.push(b.clone());
                }
            }
        }
        if !blockers.is_empty() {
            steps.push(SchemeStep {
                kind: "clear".into(),
                hint: None,
                squares: blockers,
                // Trading is a real move each, but how many depends on the
                // opponent; claiming a precise count here would be a lie.
                moves: 0,
            });
        }

        // Stage 2 — the ways in, cheapest first.
        let mut routes: Vec<&&Maneuver> = group.iter().collect();
        routes.sort_by_key(|m| m.moves);
        for m in &routes {
            steps.push(SchemeStep {
                kind: "maneuver".into(),
                hint: None,
                squares: std::iter::once(m.from.clone())
                    .chain(m.via.iter().cloned())
                    .chain([m.to.clone()])
                    .collect(),
                moves: m.moves,
            });
        }

        // Stage 3 — what the square is FOR. Only same-side plans on the
        // same square: a plan belonging to the opponent is not our payoff.
        let mut seen_hints: Vec<String> = Vec::new();
        for c in composites.iter().filter(|c| c.target == target) {
            if c.favors != favors && c.favors != Favors::Balanced {
                continue;
            }
            for hint in c
                .hints
                .iter()
                .filter(|h| EXPLOIT_HINTS.contains(&h.as_str()))
            {
                if seen_hints.contains(hint) {
                    continue;
                }
                seen_hints.push(hint.clone());
                steps.push(SchemeStep {
                    kind: "exploit".into(),
                    hint: Some(hint.clone()),
                    squares: c.squares.clone(),
                    moves: 0,
                });
            }
        }

        // A way in with nothing to do on arrival is not a sequence — the
        // Maneuver record already says everything there is to say.
        if seen_hints.is_empty() {
            continue;
        }
        // Alternative routes are not additive: the horizon is the cheapest
        // way in, not the sum of every way in.
        let horizon = routes.first().map(|m| m.moves).unwrap_or(0);
        out.push(Scheme {
            target,
            favors,
            steps,
            horizon,
        });
    }

    // Most-supported first; among equals, the one you can start soonest.
    out.sort_by_key(|s| (std::cmp::Reverse(s.steps.len()), s.horizon));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::ImbalanceKind;

    fn maneuver(to: &str, blocked: &[&str]) -> Maneuver {
        Maneuver {
            piece: "knight".into(),
            from: "c3".into(),
            via: vec![],
            to: to.into(),
            moves: 1,
            reason: "permanent_hole".into(),
            blocked_by: blocked.iter().map(|s| s.to_string()).collect(),
            favors: Favors::White,
        }
    }

    fn composite(target: &str, hints: &[&str], favors: Favors) -> CompositePlan {
        CompositePlan {
            target: target.into(),
            hints: hints.iter().map(|s| s.to_string()).collect(),
            supporting: vec![ImbalanceKind::PawnStructure],
            squares: vec![target.into()],
            score: 2,
            favors,
        }
    }

    /// HTRYC ex. 60 in full: the defenders of d5 come off FIRST, the
    /// knight lands SECOND, the backward pawn behind it is the payoff.
    /// The order is the entire lesson.
    #[test]
    fn prerequisites_come_before_the_maneuver() {
        let s = synthesize(
            &[maneuver("d5", &["f6"])],
            &[composite("d5", &["PressureBackwardPawn"], Favors::White)],
        );
        assert_eq!(s.len(), 1);
        let kinds: Vec<&str> = s[0].steps.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, vec!["clear", "maneuver", "exploit"]);
        assert_eq!(s[0].steps[0].squares, vec!["f6"]);
        assert_eq!(s[0].target, "d5");
    }

    /// An uncontested destination has no prerequisite — do not invent one.
    #[test]
    fn uncontested_destination_starts_with_the_maneuver() {
        let s = synthesize(
            &[maneuver("d5", &[])],
            &[composite("d5", &["PressureBackwardPawn"], Favors::White)],
        );
        let kinds: Vec<&str> = s[0].steps.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, vec!["maneuver", "exploit"]);
    }

    /// A lone reroute with nothing to cash in on is not a scheme — the
    /// Maneuver record already says everything there is to say.
    #[test]
    fn a_single_stage_is_not_a_scheme() {
        assert!(synthesize(&[maneuver("d5", &[])], &[]).is_empty());
    }

    /// A knight and a bishop that both want d5 are two ways into ONE
    /// plan. Listing the plan twice reads as two ideas when there is one.
    #[test]
    fn two_pieces_wanting_one_square_make_one_scheme() {
        let mut bishop = maneuver("d5", &["f6"]);
        bishop.piece = "bishop".into();
        bishop.from = "f1".into();
        bishop.via = vec!["c4".into()];
        bishop.moves = 2;
        let s = synthesize(
            &[maneuver("d5", &["f6"]), bishop],
            &[composite("d5", &["PressureBackwardPawn"], Favors::White)],
        );
        assert_eq!(s.len(), 1, "{s:?}");
        let kinds: Vec<&str> = s[0].steps.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, vec!["clear", "maneuver", "maneuver", "exploit"]);
        // The blocker is named once, not once per route.
        assert_eq!(s[0].steps[0].squares, vec!["f6"]);
        // Alternative routes are not additive: cheapest way in wins.
        assert_eq!(s[0].horizon, 1);
        assert_eq!(s[0].steps[1].moves, 1, "cheapest route listed first");
    }

    /// The opponent's plan on our square is not our payoff.
    #[test]
    fn enemy_plans_on_the_same_square_are_not_our_payoff() {
        let s = synthesize(
            &[maneuver("d5", &[])],
            &[composite("d5", &["PressureBackwardPawn"], Favors::Black)],
        );
        assert!(s.is_empty(), "{s:?}");
    }

    /// Getting-there hints are not cashing-in hints: a second reroute
    /// aimed at the same square must not be narrated as the payoff.
    #[test]
    fn only_exploit_hints_become_the_payoff() {
        let s = synthesize(
            &[maneuver("d5", &[])],
            &[composite("d5", &["ManeuverKnightToOutpost"], Favors::White)],
        );
        assert!(s.is_empty(), "{s:?}");
    }
}
