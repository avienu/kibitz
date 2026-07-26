//! PlayerProfile (docs/SILMAN_ENGINE_SPEC.md, silman-profile section).
//!
//! Pure aggregation over per-ply data the app layer extracts: WSUI alerts
//! before/after each move (the motif matrix's engine-free proxy), engine
//! evals where batch jobs supplied them, phase tags, and per-game
//! structure flags. No I/O, no chess library, no engine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fingerprint::{Color, GameScore};

/// Alert kinds mirrored from silman-core (kept as strings so this crate
/// stays decoupled from the record types' evolution).
pub type MotifKind = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseTag {
    Opening,
    Middlegame,
    Endgame,
}

/// One mainline ply, from the subject player's perspective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePly {
    /// 1-based mainline ply of the move.
    pub ply: u16,
    pub subject_moved: bool,
    pub phase: PhaseTag,
    /// Medium+ alerts in the position the mover FACED (kind, is_against_subject).
    pub alerts_before: Vec<(MotifKind, bool)>,
    /// Medium+ alerts after the move was played.
    pub alerts_after: Vec<(MotifKind, bool)>,
    /// Evals in centipawns from the SUBJECT's point of view, where known.
    pub eval_before: Option<i32>,
    pub eval_after: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileGame {
    /// Opaque id for drill-down (database game id).
    pub game_id: i64,
    pub color: Color,
    pub score: GameScore,
    pub eco: Option<String>,
    /// Structure flags observed for this game (sampled mid-game), e.g.
    /// "own-isolated-pawn", "opp-bad-bishop".
    pub structure_flags: Vec<String>,
    pub plies: Vec<ProfilePly>,
}

#[derive(Debug, Default, Serialize)]
pub struct PhaseAcpl {
    pub moves: u32,
    pub acpl: f64,
    pub blunders: u32,
    pub mistakes: u32,
    pub inaccuracies: u32,
}

#[derive(Debug, Serialize)]
pub struct MotifRow {
    pub kind: MotifKind,
    /// Times the subject faced an exploitable enemy weakness.
    pub opportunities: u32,
    pub taken: u32,
    pub missed: u32,
    /// Times the subject's own move created this weakness against them.
    pub allowed: u32,
    pub example_missed: Vec<i64>,
    pub example_allowed: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct StructureRow {
    pub flag: String,
    pub games: u32,
    pub score_pct: f64,
    pub examples: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct EcoRow {
    pub eco: String,
    pub games: u32,
    pub score_pct: f64,
    pub examples: Vec<i64>,
}

#[derive(Debug, Default, Serialize)]
pub struct Conversion {
    /// Games in which the subject's eval first reached >= +2.00.
    pub winning_reached: u32,
    pub converted_wins: u32,
    /// Games in which the subject's eval reached <= -1.00.
    pub losing_reached: u32,
    pub held: u32,
}

#[derive(Debug, Serialize)]
pub struct PlayerProfile {
    pub player: String,
    pub games: u32,
    pub score_pct: f64,
    pub eval_coverage_pct: f64,
    pub acpl_opening: PhaseAcpl,
    pub acpl_middlegame: PhaseAcpl,
    pub acpl_endgame: PhaseAcpl,
    pub motifs: Vec<MotifRow>,
    pub structures: Vec<StructureRow>,
    pub eco: Vec<EcoRow>,
    pub conversion: Conversion,
}

fn pct(n: f64, d: u32) -> f64 {
    if d == 0 {
        0.0
    } else {
        (n / d as f64 * 1000.0).round() / 10.0
    }
}

/// Aggregate the profile. Deterministic; examples capped at 3 per cell.
pub fn player_profile(player: &str, games: &[ProfileGame]) -> PlayerProfile {
    let mut points = 0.0;
    let mut phase_acc: BTreeMap<&'static str, (f64, u32, u32, u32, u32)> = BTreeMap::new();
    let mut motif: BTreeMap<MotifKind, MotifRow> = BTreeMap::new();
    let mut structures: BTreeMap<String, (u32, f64, Vec<i64>)> = BTreeMap::new();
    let mut eco_map: BTreeMap<String, (u32, f64, Vec<i64>)> = BTreeMap::new();
    let mut conversion = Conversion::default();
    let mut evaled_moves = 0u32;
    let mut subject_moves = 0u32;

    for g in games {
        points += g.score.points();

        // ECO + structure per game.
        let eco = g
            .eco
            .clone()
            .map(|e| e.chars().take(3).collect::<String>())
            .unwrap_or_else(|| "?".into());
        let e = eco_map.entry(eco).or_default();
        e.0 += 1;
        e.1 += g.score.points();
        if e.2.len() < 3 {
            e.2.push(g.game_id);
        }
        for flag in &g.structure_flags {
            let s = structures.entry(flag.clone()).or_default();
            s.0 += 1;
            s.1 += g.score.points();
            if s.2.len() < 3 {
                s.2.push(g.game_id);
            }
        }

        // Conversion / defense from the eval trace.
        let mut reached_winning = false;
        let mut reached_losing = false;
        for p in &g.plies {
            if let Some(e) = p.eval_after.or(p.eval_before) {
                if e >= 200 {
                    reached_winning = true;
                }
                if e <= -100 {
                    reached_losing = true;
                }
            }
        }
        if reached_winning {
            conversion.winning_reached += 1;
            if g.score == GameScore::Win {
                conversion.converted_wins += 1;
            }
        }
        if reached_losing {
            conversion.losing_reached += 1;
            if g.score != GameScore::Loss {
                conversion.held += 1;
            }
        }

        // Per-move: ACPL and motifs (subject's moves only). A weakness
        // that persists across several moves (a weak king does not heal)
        // is ONE opportunity, counted when it first appears and re-armed
        // only after it disappears.
        let mut persisting: std::collections::BTreeSet<MotifKind> = Default::default();
        for p in &g.plies {
            if !p.subject_moved {
                continue;
            }
            subject_moves += 1;
            if let (Some(before), Some(after)) = (p.eval_before, p.eval_after) {
                evaled_moves += 1;
                let loss = (before - after).max(0) as f64;
                let key = match p.phase {
                    PhaseTag::Opening => "opening",
                    PhaseTag::Middlegame => "middlegame",
                    PhaseTag::Endgame => "endgame",
                };
                let acc = phase_acc.entry(key).or_default();
                acc.0 += loss.min(1000.0); // cap mate-swings
                acc.1 += 1;
                if loss >= 200.0 {
                    acc.2 += 1;
                } else if loss >= 100.0 {
                    acc.3 += 1;
                } else if loss >= 50.0 {
                    acc.4 += 1;
                }
            }

            // Motifs. Opportunity: enemy weakness in the faced position,
            // newly arisen since the subject's previous turn.
            for (kind, against_subject) in &p.alerts_before {
                if *against_subject || persisting.contains(kind) {
                    continue;
                }
                let row = motif.entry(kind.clone()).or_insert_with(|| MotifRow {
                    kind: kind.clone(),
                    opportunities: 0,
                    taken: 0,
                    missed: 0,
                    allowed: 0,
                    example_missed: vec![],
                    example_allowed: vec![],
                });
                row.opportunities += 1;
                let still_there = p.alerts_after.iter().any(|(k, vs)| k == kind && !*vs);
                if still_there {
                    row.missed += 1;
                    if row.example_missed.len() < 3 && !row.example_missed.contains(&g.game_id) {
                        row.example_missed.push(g.game_id);
                    }
                } else {
                    row.taken += 1;
                }
            }
            // Re-arm bookkeeping: whatever enemy weaknesses remain after
            // this move keep their "already counted" status.
            persisting = p
                .alerts_after
                .iter()
                .filter(|(_, vs)| !*vs)
                .map(|(k, _)| k.clone())
                .collect();

            // Allowed: a NEW weakness against the subject after their move.
            for (kind, against_subject) in &p.alerts_after {
                if !*against_subject {
                    continue;
                }
                let was_there = p.alerts_before.iter().any(|(k, vs)| k == kind && *vs);
                if !was_there {
                    let row = motif.entry(kind.clone()).or_insert_with(|| MotifRow {
                        kind: kind.clone(),
                        opportunities: 0,
                        taken: 0,
                        missed: 0,
                        allowed: 0,
                        example_missed: vec![],
                        example_allowed: vec![],
                    });
                    row.allowed += 1;
                    if row.example_allowed.len() < 3 && !row.example_allowed.contains(&g.game_id) {
                        row.example_allowed.push(g.game_id);
                    }
                }
            }
        }
    }

    let phase = |k: &str| -> PhaseAcpl {
        let (sum, n, b, m, i) = phase_acc.get(k).copied().unwrap_or_default();
        PhaseAcpl {
            moves: n,
            acpl: if n == 0 {
                0.0
            } else {
                (sum / n as f64 * 10.0).round() / 10.0
            },
            blunders: b,
            mistakes: m,
            inaccuracies: i,
        }
    };

    let mut motifs: Vec<MotifRow> = motif.into_values().collect();
    motifs.sort_by_key(|r| std::cmp::Reverse(r.missed + r.allowed));
    let mut structures: Vec<StructureRow> = structures
        .into_iter()
        .map(|(flag, (n, pts, ex))| StructureRow {
            flag,
            games: n,
            score_pct: pct(pts, n),
            examples: ex,
        })
        .collect();
    structures.sort_by_key(|r| std::cmp::Reverse(r.games));
    let mut eco: Vec<EcoRow> = eco_map
        .into_iter()
        .map(|(eco, (n, pts, ex))| EcoRow {
            eco,
            games: n,
            score_pct: pct(pts, n),
            examples: ex,
        })
        .collect();
    eco.sort_by_key(|r| std::cmp::Reverse(r.games));

    PlayerProfile {
        player: player.to_string(),
        games: games.len() as u32,
        score_pct: pct(points, games.len() as u32),
        eval_coverage_pct: pct(evaled_moves as f64, subject_moves.max(1)),
        acpl_opening: phase("opening"),
        acpl_middlegame: phase("middlegame"),
        acpl_endgame: phase("endgame"),
        motifs,
        structures,
        eco,
        conversion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ply(
        ply: u16,
        subject: bool,
        before: &[(&str, bool)],
        after: &[(&str, bool)],
        evals: Option<(i32, i32)>,
    ) -> ProfilePly {
        ProfilePly {
            ply,
            subject_moved: subject,
            phase: PhaseTag::Middlegame,
            alerts_before: before.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            alerts_after: after.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            eval_before: evals.map(|(b, _)| b),
            eval_after: evals.map(|(_, a)| a),
        }
    }

    /// Every number below is computable by inspection:
    /// - game 1: subject faces an Undefended opportunity and TAKES it
    ///   (alert gone after); evals 0→+300 (no loss), then +300→+250
    ///   (loss 50 = inaccuracy). Reaches +2.00 ⇒ winning_reached; Win ⇒
    ///   converted.
    /// - game 2: subject faces the same opportunity and MISSES it (alert
    ///   persists), then ALLOWS a TrappedPiece against itself; evals
    ///   0→-250 (blunder, loss 250). Reaches -1.00 ⇒ losing_reached;
    ///   Loss ⇒ not held.
    #[test]
    fn hand_computable_profile() {
        let games = vec![
            ProfileGame {
                game_id: 11,
                color: Color::White,
                score: GameScore::Win,
                eco: Some("B01".into()),
                structure_flags: vec!["own-passed-pawn".into()],
                plies: vec![
                    ply(1, true, &[("Undefended", false)], &[], Some((0, 300))),
                    ply(2, false, &[], &[], None),
                    ply(3, true, &[], &[], Some((300, 250))),
                ],
            },
            ProfileGame {
                game_id: 22,
                color: Color::Black,
                score: GameScore::Loss,
                eco: Some("B01".into()),
                structure_flags: vec!["own-isolated-pawn".into()],
                plies: vec![
                    ply(
                        1,
                        true,
                        &[("Undefended", false)],
                        &[("Undefended", false)],
                        Some((0, -250)),
                    ),
                    ply(2, false, &[], &[], None),
                    // The same loose piece is STILL there: persisting
                    // weaknesses are one opportunity, not one per move.
                    ply(
                        3,
                        true,
                        &[("Undefended", false)],
                        &[("TrappedPiece", true)],
                        None,
                    ),
                ],
            },
        ];
        let p = player_profile("Subject", &games);
        assert_eq!(p.games, 2);
        assert_eq!(p.score_pct, 50.0);

        // ACPL (middlegame): losses 0, 50, 250 over 3 evaled moves = 100.0.
        assert_eq!(p.acpl_middlegame.moves, 3);
        assert_eq!(p.acpl_middlegame.acpl, 100.0);
        assert_eq!(p.acpl_middlegame.blunders, 1);
        assert_eq!(p.acpl_middlegame.mistakes, 0);
        assert_eq!(p.acpl_middlegame.inaccuracies, 1);
        // 3 of 4 subject moves had eval pairs.
        assert_eq!(p.eval_coverage_pct, 75.0);

        // Motif matrix.
        let und = p.motifs.iter().find(|m| m.kind == "Undefended").unwrap();
        assert_eq!(
            (und.opportunities, und.taken, und.missed, und.allowed),
            (2, 1, 1, 0)
        );
        assert_eq!(und.example_missed, vec![22]);
        let trap = p.motifs.iter().find(|m| m.kind == "TrappedPiece").unwrap();
        assert_eq!((trap.opportunities, trap.allowed), (0, 1));
        assert_eq!(trap.example_allowed, vec![22]);

        // Structure + ECO with drill-down examples.
        let iso = p
            .structures
            .iter()
            .find(|s| s.flag == "own-isolated-pawn")
            .unwrap();
        assert_eq!((iso.games, iso.score_pct), (1, 0.0));
        assert_eq!(iso.examples, vec![22]);
        assert_eq!(p.eco[0].eco, "B01");
        assert_eq!(p.eco[0].games, 2);

        // Conversion & defense.
        assert_eq!(p.conversion.winning_reached, 1);
        assert_eq!(p.conversion.converted_wins, 1);
        assert_eq!(p.conversion.losing_reached, 1);
        assert_eq!(p.conversion.held, 0);
    }
}
