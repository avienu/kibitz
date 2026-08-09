//! Schematic thinking: turn converging plans into ORDERED sequences.
//!
//! [`crate::plans`] answers a spatial question — which hints point at the
//! same square. That is a taxonomy, and a taxonomy cannot express the
//! thing the books actually teach: *first* trade the defenders of d5,
//! *then* land the knight, *then* press the weakness behind it (Jeremy
//! Jeremy Silman, How to Reassess Your Chess, ex. 60). Nimzowitsch's
//! restrain-blockade-destroy is the same shape — one plan, three stages,
//! and the order is the whole lesson.
//!
//! A scheme is emitted only where there is genuine sequence. A plan with
//! a single stage is already fully described by its `CompositePlan`, and
//! wrapping it in ceremony would just be noise.

use cozy_chess::{Board, Color, Piece, Square};

use crate::record::{CompositePlan, Favors, Maneuver, Scheme, SchemeStep};

fn parse_sq(s: &str) -> Option<Square> {
    s.parse().ok()
}

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

/// Who can get at `blocker`, and at what cost? Prefers a piece that is
/// already bearing down on it, then the shortest route. Pieces listed in
/// `reserved` are spoken for (they are the ones we mean to land on the
/// target) and are only drafted as a last resort.
fn find_clearer(
    board: &Board,
    color: Color,
    blocker: Square,
    reserved: &[Square],
) -> Option<(Square, Vec<Square>, u8)> {
    let mut best: Option<(Square, Vec<Square>, u8)> = None;
    for piece in [Piece::Bishop, Piece::Knight, Piece::Rook, Piece::Queen] {
        for from in board.colored_pieces(color, piece) {
            let Some(r) = crate::route::route_to_attack(board, color, piece, from, blocker) else {
                continue;
            };
            // Already attacking costs nothing; otherwise the walk.
            let cost = if r.to == from { 0 } else { r.moves() };
            // A reserved piece can do the job, but it is then spent and
            // cannot also occupy the square — rank it behind everyone else.
            let penalty = u8::from(reserved.contains(&from)) * 10;
            let score = cost.saturating_add(penalty);
            if best.as_ref().is_none_or(|(_, _, b)| score < *b) {
                let via = if r.to == from {
                    Vec::new()
                } else {
                    r.via.iter().copied().chain([r.to]).collect()
                };
                best = Some((from, via, score));
            }
        }
    }
    best.map(|(from, via, score)| (from, via, score % 10))
}

/// Build the ordered sequences implied by the maneuvers and the composite
/// plans that share their target squares. Longest-horizon plans are not
/// "best" — schemes are ranked by how much converging support they have,
/// then by being cheap to start.
pub fn synthesize(
    board: &Board,
    maneuvers: &[Maneuver],
    composites: &[CompositePlan],
) -> Vec<Scheme> {
    // One scheme per square, not per piece — but the pieces that want it
    // are not merely alternatives. One of them may be the piece that
    // trades the defender off so another can settle there unchallenged,
    // which is a DIVISION OF LABOUR inside a single plan, not two plans.
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
        // Who clears? The pieces that want the square are NOT merely
        // alternative ways in: one of them may be the piece that removes
        // the defender so another can settle unchallenged (Bg5xf6 then
        // Nd5 — HTRYC ex. 60). A piece drafted to clear is spent, so it
        // drops out of the ways-in list.
        let side = match favors {
            Favors::Black => Color::Black,
            _ => Color::White,
        };
        let reserved: Vec<Square> = group.iter().filter_map(|m| parse_sq(&m.from)).collect();
        let mut spent: Vec<String> = Vec::new();
        let mut clear_agent: Option<(String, Vec<String>, u8)> = None;
        if let Some(first) = blockers.first().and_then(|b| parse_sq(b)) {
            // Only draft a reserved piece when another one is left to
            // occupy the square; a lone piece cannot both trade and stay.
            let reserve_ok = group.len() > 1;
            let pool: Vec<Square> = if reserve_ok {
                Vec::new()
            } else {
                reserved.clone()
            };
            if let Some((from, via, cost)) = find_clearer(board, side, first, &pool) {
                let name = crate::record::square_name(from);
                if reserved.contains(&from) {
                    spent.push(name.clone());
                }
                clear_agent = Some((
                    name,
                    via.into_iter().map(crate::record::square_name).collect(),
                    cost,
                ));
            }
        }
        if !blockers.is_empty() {
            let (agent, via, moves) = match clear_agent {
                Some((a, v, c)) => (Some(a), v, c),
                None => (None, Vec::new(), 0),
            };
            steps.push(SchemeStep {
                kind: "clear".into(),
                hint: None,
                agent,
                via,
                squares: blockers,
                moves,
            });
        }

        // Stage 2 — the ways in, cheapest first, minus anyone spent.
        let mut routes: Vec<&&Maneuver> =
            group.iter().filter(|m| !spent.contains(&m.from)).collect();
        routes.sort_by_key(|m| m.moves);
        for m in &routes {
            steps.push(SchemeStep {
                kind: "maneuver".into(),
                hint: None,
                agent: Some(m.from.clone()),
                via: m.via.clone(),
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
                    agent: None,
                    via: Vec::new(),
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
        // Alternative routes are not additive: the horizon is the cost of
        // clearing plus the cheapest remaining way in, not the sum of
        // every way in.
        let clear_cost = steps
            .iter()
            .find(|s| s.kind == "clear")
            .map(|s| s.moves)
            .unwrap_or(0);
        let horizon = clear_cost.saturating_add(routes.first().map(|m| m.moves).unwrap_or(0));
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
    use std::str::FromStr;

    /// The Sveshnikov tabiya: White's c3-knight and f1-bishop both want
    /// d5, and the f6 knight is the defender in the way.
    fn sveshnikov() -> Board {
        Board::from_str("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7")
            .expect("fen")
    }

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
            &sveshnikov(),
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
            &sveshnikov(),
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
        assert!(synthesize(&sveshnikov(), &[maneuver("d5", &[])], &[]).is_empty());
    }

    /// A knight and a bishop that both want d5 are one plan, not two —
    /// and they are not interchangeable either. The f6 knight defends
    /// d5, so somebody has to trade it off first, and the piece that
    /// does that job is spent (Bg5xf6 then Nd5 — HTRYC ex. 60).
    #[test]
    fn two_pieces_wanting_one_square_divide_the_labour() {
        let mut bishop = maneuver("d5", &["f6"]);
        bishop.piece = "bishop".into();
        bishop.from = "f1".into();
        bishop.via = vec!["c4".into()];
        bishop.moves = 2;
        let s = synthesize(
            &sveshnikov(),
            &[maneuver("d5", &["f6"]), bishop],
            &[composite("d5", &["PressureBackwardPawn"], Favors::White)],
        );
        assert_eq!(s.len(), 1, "one square, one plan: {s:?}");
        let kinds: Vec<&str> = s[0].steps.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, vec!["clear", "maneuver", "maneuver", "exploit"]);
        // The blocker is named once, not once per route, and a piece is
        // named to go and get it rather than the step being a wish.
        let clear = &s[0].steps[0];
        assert_eq!(clear.squares, vec!["f6"]);
        assert!(
            clear.agent.is_some(),
            "somebody must do the trading: {clear:?}"
        );
        // Horizon is clearing plus the cheapest remaining way in — the
        // prerequisite is part of the plan's cost, not free.
        assert_eq!(s[0].horizon, clear.moves + s[0].steps[1].moves);
    }

    /// The opponent's plan on our square is not our payoff.
    #[test]
    fn enemy_plans_on_the_same_square_are_not_our_payoff() {
        let s = synthesize(
            &sveshnikov(),
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
            &sveshnikov(),
            &[maneuver("d5", &[])],
            &[composite("d5", &["ManeuverKnightToOutpost"], Favors::White)],
        );
        assert!(s.is_empty(), "{s:?}");
    }
}
