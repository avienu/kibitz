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
            // A hint's TARGET is its last square (routes end at the
            // destination; blockades name the stop square).
            let Some(target_file) = plan.squares.last().and_then(|s| file_of(s)) else {
                continue; // square-less hints don't cluster
            };
            // A blockade is the DEFENDER's plan: it clusters with the side
            // facing the passer, not the side the parent imbalance favors.
            let hint_favors = match plan.hint.as_str() {
                "BlockadeWhitePasser" => Favors::Black,
                "BlockadeBlackPasser" => Favors::White,
                _ => imb.favors,
            };
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
            hint: h.into(),
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
