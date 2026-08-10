//! Plan synthesis (run-5 feedback item 4): cluster the individual
//! [`PlanHint`]s emitted by independent imbalance detectors around shared
//! targets, so "hole on d5" + "knight route to d5" + "pressure the
//! backward d6 pawn" become ONE plan with three supporting imbalances.

use std::collections::BTreeMap;

use crate::record::{CompositePlan, Favors, Imbalance, ImbalanceKind, Magnitude};

fn magnitude_weight(m: Magnitude) -> u32 {
    match m {
        Magnitude::Minor => 1,
        Magnitude::Clear => 2,
        Magnitude::Winning => 3,
    }
}

/// The clustering key for a square: the square itself plus its file, so
/// hints about d5 and d6 (same file, adjacent play) can merge.
fn file_of(sq: &str) -> Option<char> {
    let mut ch = sq.chars();
    let f = ch.next()?;
    let r = ch.next()?;
    (('a'..='h').contains(&f) && ('1'..='8').contains(&r) && ch.next().is_none()).then_some(f)
}

/// Cluster plan hints on shared target squares/files and rank composites
/// by the count and magnitude of their independent supporting imbalances.
pub fn synthesize(imbalances: &[Imbalance]) -> Vec<CompositePlan> {
    // One entry per (favors, file): plans for different sides never merge.
    #[derive(Default)]
    struct Cluster {
        hints: Vec<(String, Magnitude)>,
        supporting: Vec<ImbalanceKind>,
        squares: Vec<String>,
        score: u32,
        // Square most often referenced — becomes the named target.
        square_votes: BTreeMap<String, u32>,
    }
    let mut clusters: BTreeMap<(u8, char), Cluster> = BTreeMap::new();
    let favors_key = |f: Favors| match f {
        Favors::White => 0u8,
        Favors::Black => 1,
        Favors::Balanced => 2,
    };

    for imb in imbalances {
        for plan in &imb.plans {
            // Development-prior hints (run 11) name LOCATIONS (sleeping
            // pieces, the king's home, a wandering piece), not targets —
            // clustering them around a file produces signpost nonsense.
            // Only ClaimTheCenter carries a genuine target square.
            if crate::development::is_prior_hint(&plan.hint) && plan.hint != "ClaimTheCenter" {
                continue;
            }
            // Reroutes that own an exact destination stay out of the
            // file vote — see route::EXACT_DESTINATION_HINTS. They reach
            // the user as first-class Maneuvers instead.
            if crate::route::EXACT_DESTINATION_HINTS.contains(&plan.hint.as_str()) {
                continue;
            }
            // A hint's TARGET is its last square (routes end at the
            // destination; blockades name the stop square).
            let Some(target_file) = plan.squares.last().and_then(|s| file_of(s)) else {
                continue; // square-less hints don't cluster
            };
            // Whose plan it is, from the hint itself where it says so.
            let hint_favors = plan.attributed(imb.favors);
            let c = clusters
                .entry((favors_key(hint_favors), target_file))
                .or_default();
            if !c.hints.iter().any(|(h, _)| h == &plan.hint) {
                c.hints.push((plan.hint.clone(), imb.magnitude));
            }
            if !c.supporting.contains(&imb.kind) {
                c.supporting.push(imb.kind);
                c.score += magnitude_weight(imb.magnitude);
            }
            for (i, sq) in plan.squares.iter().enumerate() {
                if !c.squares.contains(sq) {
                    c.squares.push(sq.clone());
                }
                // Destination squares get double vote weight.
                let w = if i + 1 == plan.squares.len() { 2 } else { 1 };
                *c.square_votes.entry(sq.clone()).or_default() += w;
            }
        }
    }

    let mut out: Vec<CompositePlan> = clusters
        .into_iter()
        .map(|((fk, file), mut c)| {
            c.hints.sort_by_key(|(_, m)| std::cmp::Reverse(*m));
            let target = c
                .square_votes
                .iter()
                .max_by_key(|(sq, n)| (**n, std::cmp::Reverse(sq.as_str())))
                .map(|(sq, _)| sq.clone())
                .unwrap_or_else(|| format!("{file}-file"));
            CompositePlan {
                target,
                hints: c.hints.into_iter().map(|(h, _)| h).collect(),
                supporting: c.supporting,
                squares: c.squares,
                score: c.score,
                favors: match fk {
                    0 => Favors::White,
                    1 => Favors::Black,
                    _ => Favors::Balanced,
                },
            }
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse((p.supporting.len() as u32, p.score)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::PlanHint;
    use std::collections::BTreeMap as Map;

    fn imb(kind: ImbalanceKind, mag: Magnitude, plans: Vec<PlanHint>) -> Imbalance {
        Imbalance {
            kind,
            favors: Favors::White,
            magnitude: mag,
            evidence: Map::new(),
            plans,
        }
    }

    fn hint(h: &str, sqs: &[&str]) -> PlanHint {
        PlanHint {
            speed: None,
            hint: h.into(),
            owner: None,
            squares: sqs.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Textbook convergence: an outpost, a knight route, and pressure on
    /// the backward pawn behind it all point at d5/d6 — one plan with
    /// three supports, outranking a lone majority hint. (Pattern: the
    /// Sveshnikov d5 complex; see imbalance_golden.rs for the real-FEN
    /// version.)
    #[test]
    fn convergent_hints_merge_and_outrank() {
        let imbalances = vec![
            imb(
                ImbalanceKind::SquaresOutposts,
                Magnitude::Clear,
                vec![hint("ManeuverKnightToOutpost", &["c3", "d5"])],
            ),
            imb(
                ImbalanceKind::PawnStructure,
                Magnitude::Clear,
                vec![
                    hint("PressureBackwardPawn", &["d6", "d5"]),
                    hint("AdvanceQueensideMajority", &["b4"]),
                ],
            ),
            imb(
                ImbalanceKind::FilesDiagonals,
                Magnitude::Minor,
                vec![hint("DoubleOnOpenFile", &["d1"])],
            ),
        ];
        let plans = synthesize(&imbalances);
        assert!(plans.len() >= 2);
        let top = &plans[0];
        assert_eq!(top.target, "d5");
        assert_eq!(top.supporting.len(), 3, "{top:?}");
        assert!(top.hints.contains(&"ManeuverKnightToOutpost".to_string()));
        assert!(top.hints.contains(&"PressureBackwardPawn".to_string()));
        assert!(top.score >= 5);
        // The lone b4 majority hint is a separate, lower-ranked plan.
        let runner = &plans[1];
        assert_eq!(runner.supporting.len(), 1);
    }

    #[test]
    fn different_sides_never_merge() {
        let mut a = imb(
            ImbalanceKind::SquaresOutposts,
            Magnitude::Clear,
            vec![hint("ManeuverKnightToOutpost", &["d5"])],
        );
        a.favors = Favors::White;
        let mut b = imb(
            ImbalanceKind::PawnStructure,
            Magnitude::Clear,
            vec![hint("BlockadeWhitePasser", &["d6"])],
        );
        b.favors = Favors::Black;
        let plans = synthesize(&[a, b]);
        assert_eq!(plans.len(), 2);
        assert!(plans.iter().all(|p| p.supporting.len() == 1));
    }
}

/// The plan-speed post-pass (run 12): give every hint the engine can
/// honestly cost a `speed` — moves the OWNER needs to complete or
/// activate the plan.
///
/// One pass over the finished imbalances, never at the emission sites,
/// so speed cannot change what fires and is testable in isolation. The
/// tempo comparison in `suggest::role_of` and the maintainer's
/// prophylaxis hypothesis both need a per-side plan speed, and
/// horizon-study measured the previous source (schemes) at 0-1%
/// both-sides coverage.
///
/// Families with no honest cheap estimate keep `None`: maintenance
/// plans (Keep*, Restrict*, UseSpace*, Overprotect*) have no arrival
/// time, and None is the correct value there, not zero — otherwise
/// every side would trivially own a horizon of 0.
pub fn annotate_speed(board: &cozy_chess::Board, imbalances: &mut [Imbalance]) {
    use cozy_chess::{Color, Piece, Square};
    let parse = |s: &str| -> Option<Square> { s.parse().ok() };
    let color_of = |f: Favors| match f {
        Favors::White => Some(Color::White),
        Favors::Black => Some(Color::Black),
        Favors::Balanced => None,
    };
    // Cheapest way for `color` to bring a NEW attacker against `target`:
    // 1 if something already attacks it, else the shortest attack route
    // of any piece. None when nothing can come.
    let attack_cost = |color: Color, target: Square| -> Option<u8> {
        if !crate::attack::attackers_of(board, target, color, board.occupied()).is_empty() {
            return Some(1);
        }
        let mut best: Option<u8> = None;
        for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
            for from in board.colored_pieces(color, piece) {
                if let Some(r) = crate::route::route_to_attack(board, color, piece, from, target) {
                    let m = r.moves();
                    if best.is_none_or(|b| m < b) {
                        best = Some(m);
                    }
                }
            }
        }
        best
    };
    // Cheapest piece journey onto `target` itself (blockade duty).
    let occupy_cost = |color: Color, target: Square| -> Option<u8> {
        if (board.colors(color) & target.bitboard()).is_empty() {
            let mut best: Option<u8> = None;
            for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
                for from in board.colored_pieces(color, piece) {
                    if let Some(r) = crate::route::route_to(
                        board,
                        color,
                        piece,
                        from,
                        target.bitboard(),
                        &|_| true,
                    ) {
                        let m = r.moves();
                        if best.is_none_or(|b| m < b) {
                            best = Some(m);
                        }
                    }
                }
            }
            best
        } else {
            Some(0)
        }
    };

    for imb in imbalances.iter_mut() {
        let parent = imb.favors;
        for plan in imb.plans.iter_mut() {
            let owner = crate::record::attribute(&plan.hint, plan.owner, parent);
            let Some(color) = color_of(owner) else {
                continue;
            };
            let sq = |i: usize| plan.squares.get(i).and_then(|s| parse(s));
            plan.speed = match plan.hint.as_str() {
                // Routed maneuvers carry their own path.
                "ManeuverKnightToOutpost"
                | "ManeuverBishopToSupportPoint"
                | "ManeuverRookToOpenFile"
                    if plan.squares.len() >= 2 =>
                {
                    Some((plan.squares.len() - 1).min(6) as u8)
                }
                // Pressure family: the plan is executing once one more
                // attacker bears on the target.
                "TargetWeakPawn" | "PressureDoubledPawn" => {
                    sq(0).and_then(|t| attack_cost(color, t))
                }
                "PressureBackwardPawn" | "BlockadeThenPressure" => {
                    sq(0).and_then(|t| attack_cost(color, t))
                }
                // Trade family: get at the named enemy piece.
                "HuntBishopPair" | "TradeSquareDefender" => {
                    // squares = [ours/theirs, victim] — the victim is the
                    // last square for HuntBishopPair, the first for
                    // TradeSquareDefender; both are enemy-occupied, so
                    // take whichever square holds an enemy piece.
                    let victim = [sq(0), sq(1)]
                        .into_iter()
                        .flatten()
                        .find(|s| !(board.colors(!color) & s.bitboard()).is_empty());
                    victim.and_then(|v| attack_cost(color, v))
                }
                "TradeOffAttacker" | "UprootBlockader" => sq(0).and_then(|v| attack_cost(color, v)),
                // Blockade duty: somebody has to reach the stop square.
                "BlockadeWhitePasser" | "BlockadeBlackPasser" => {
                    sq(0).and_then(|t| occupy_cost(color, t))
                }
                "RookBehindPasser" => sq(1).and_then(|t| {
                    if (board.colored_pieces(color, Piece::Rook) & t.bitboard()).is_empty() {
                        occupy_cost(color, t)
                    } else {
                        Some(0)
                    }
                }),
                // Rooks already established on the seventh.
                "RookToSeventh" => Some(0),
                // King work.
                "ActivateKingInEndgame" => sq(0).map(|t| {
                    let k = board.king(color);
                    (k.file() as i8 - t.file() as i8)
                        .abs()
                        .max((k.rank() as i8 - t.rank() as i8).abs())
                        .clamp(0, 6) as u8
                }),
                _ => None,
            };
            // A journey longer than the routing horizon is a dream, not
            // a speed.
            if plan.speed.is_some_and(|s| s > 6) {
                plan.speed = None;
            }
        }
    }
}

#[cfg(test)]
mod speed_tests {
    use crate::record::Favors;
    use std::str::FromStr;

    type SpeedRow = (String, Option<Favors>, Vec<String>, Option<u8>);

    fn speeds(fen: &str) -> Vec<SpeedRow> {
        let board = cozy_chess::Board::from_str(fen).expect("fen");
        let mut imb = crate::imbalance::assess(&board);
        super::annotate_speed(&board, &mut imb);
        imb.into_iter()
            .flat_map(|i| {
                i.plans
                    .into_iter()
                    .map(move |p| (p.hint, p.owner, p.squares, p.speed))
            })
            .collect()
    }

    /// The three spot-checks fixed in the plan-speed prediction sheet,
    /// on the positions that fixed them (Chess Praxis game 70; CBoCS
    /// p. 192 endgame).
    #[test]
    fn plan_speed_spot_checks() {
        let g70 = speeds("3r4/bp1r1k2/2p1b2p/1p2P1p1/3P4/P4NKP/1PBR2P1/4R3 w - - 1 31");
        // (a) a rook one move from standing behind its passer: speed 0-1.
        let rbp = g70.iter().find(|p| p.0 == "RookBehindPasser").expect("rbp");
        assert!(rbp.3.is_some_and(|s| s <= 1), "{rbp:?}");
        // (c) pressure plans whose target is already attacked: speed 1.
        let pbp = g70
            .iter()
            .find(|p| p.0 == "PressureBackwardPawn")
            .expect("pbp");
        assert_eq!(pbp.3, Some(1), "{pbp:?}");
        // Routed maneuvers keep their own path length.
        let mko = g70
            .iter()
            .find(|p| p.0 == "ManeuverKnightToOutpost")
            .expect("mko");
        assert_eq!(mko.3, Some((mko.2.len() - 1) as u8), "{mko:?}");
        // A blockade already manned costs its owner nothing.
        let bwp = g70
            .iter()
            .find(|p| p.0 == "BlockadeWhitePasser")
            .expect("bwp");
        assert_eq!(bwp.3, Some(0), "{bwp:?}");

        // (b) king activation speed equals the king's chebyshev distance.
        let end = speeds("1rB5/1P6/p4k2/2p5/2P2KP1/8/8/8 w - - 0 1");
        for p in end.iter().filter(|p| p.0 == "ActivateKingInEndgame") {
            assert_eq!(p.3, Some(1), "{p:?}"); // both kings one step off-center
        }
    }

    /// Maintenance plans have no arrival time: None, never zero.
    #[test]
    fn maintenance_plans_have_no_speed() {
        let g35 = speeds("r4rk1/p1pqbppp/B1p1pn2/2Pp3b/Q2P1B2/P3P3/1P3PPP/RN2K2R b KQ - 2 12");
        for p in g35.iter().filter(|p| {
            matches!(
                p.0.as_str(),
                "KeepPositionClosed" | "UseSpaceAvoidExchanges" | "RestrictKnight"
            )
        }) {
            assert_eq!(p.3, None, "{p:?}");
        }
    }
}
