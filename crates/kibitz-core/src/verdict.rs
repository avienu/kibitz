//! Who stands better: one weighted vote across the detected imbalances.
//!
//! This used to live inside the book-eval harness, which meant the
//! product had no opinion of its own — the only "who is better" answer
//! in the codebase existed to be scored. It belongs here, where the app
//! can show it and where it can be FITTED against data rather than
//! guessed at.
//!
//! The weights below are per-imbalance-kind because a uniform vote is
//! obviously wrong: a Minor lean in Development is not the same claim as
//! a Minor lean in Material, and treating them alike is a large part of
//! why the favors axis sits where it does (docs/VALIDATION.md).

use crate::record::{Favors, Imbalance, ImbalanceKind, Magnitude};

/// Weight per imbalance kind, in vote units.
///
/// Fitted against decisive master games — see `kibitz-cli favors-fit`
/// and the table in docs/VALIDATION.md. Hand-editing these without
/// re-running the fit is how a tuned model turns back into a guess.
pub const KIND_WEIGHT: [(ImbalanceKind, i32); 8] = [
    (ImbalanceKind::Material, 24),
    (ImbalanceKind::PawnStructure, 2),
    (ImbalanceKind::MinorPieces, 18),
    (ImbalanceKind::SquaresOutposts, 4),
    (ImbalanceKind::FilesDiagonals, 6),
    (ImbalanceKind::Space, 10),
    (ImbalanceKind::Development, 6),
    (ImbalanceKind::Initiative, 10),
];

pub fn weight_of(kind: ImbalanceKind) -> i32 {
    KIND_WEIGHT
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, w)| *w)
        .unwrap_or(10)
}

/// How much a magnitude multiplies its kind's weight.
pub fn magnitude_factor(m: Magnitude) -> i32 {
    match m {
        Magnitude::Minor => 1,
        Magnitude::Clear => 2,
        Magnitude::Winning => 4,
    }
}

/// The signed lean: positive favours White.
pub fn lean(imbalances: &[Imbalance]) -> i32 {
    imbalances
        .iter()
        .map(|i| {
            let w = weight_of(i.kind) * magnitude_factor(i.magnitude);
            match i.favors {
                Favors::White => w,
                Favors::Black => -w,
                Favors::Balanced => 0,
            }
        })
        .sum()
}

/// Who stands better, and by how much.
pub fn overall_favors(imbalances: &[Imbalance]) -> (Favors, i32) {
    let lean = lean(imbalances);
    let side = match lean.cmp(&0) {
        std::cmp::Ordering::Greater => Favors::White,
        std::cmp::Ordering::Less => Favors::Black,
        std::cmp::Ordering::Equal => Favors::Balanced,
    };
    (side, lean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn imb(kind: ImbalanceKind, favors: Favors, magnitude: Magnitude) -> Imbalance {
        Imbalance {
            kind,
            favors,
            magnitude,
            evidence: BTreeMap::new(),
            plans: Vec::new(),
        }
    }

    #[test]
    fn a_winning_edge_outvotes_two_minor_ones() {
        let v = vec![
            imb(ImbalanceKind::Material, Favors::White, Magnitude::Winning),
            imb(ImbalanceKind::Space, Favors::Black, Magnitude::Minor),
            imb(ImbalanceKind::Development, Favors::Black, Magnitude::Minor),
        ];
        assert_eq!(overall_favors(&v).0, Favors::White);
    }

    /// Balanced records are evidence of a detector having looked, not of
    /// anybody standing better, and must not move the vote.
    #[test]
    fn balanced_records_do_not_vote() {
        let v = vec![
            imb(
                ImbalanceKind::Material,
                Favors::Balanced,
                Magnitude::Winning,
            ),
            imb(ImbalanceKind::Space, Favors::White, Magnitude::Minor),
        ];
        let (side, lean) = overall_favors(&v);
        assert_eq!(side, Favors::White);
        assert_eq!(lean, weight_of(ImbalanceKind::Space));
    }

    /// The fit's clearest and most stable finding, pinned so a casual
    /// edit cannot quietly undo it: material outweighs everything, and
    /// our squares/outposts reading carries little outcome signal by
    /// comparison. Both held across every seed tried.
    #[test]
    fn material_outweighs_the_positional_kinds() {
        let material = weight_of(ImbalanceKind::Material);
        for k in [
            ImbalanceKind::SquaresOutposts,
            ImbalanceKind::PawnStructure,
            ImbalanceKind::Development,
            ImbalanceKind::FilesDiagonals,
        ] {
            assert!(material > weight_of(k), "material vs {k:?}");
        }
    }

    #[test]
    fn an_empty_reading_is_balanced() {
        assert_eq!(overall_favors(&[]).0, Favors::Balanced);
    }
}
