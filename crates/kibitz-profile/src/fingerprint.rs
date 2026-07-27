//! Repertoire fingerprint: what a player actually plays, position-hash
//! (transposition-) aware, split by color, with per-ECO scores and
//! deviation-from-theory points.
//!
//! This module is pure aggregation over precomputed inputs: the caller (app
//! layer) supplies position hashes, SANs, ECO codes and a theory-position
//! set; nothing here touches a database, the network, or a chess library.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameScore {
    Win,
    Draw,
    Loss,
}

impl GameScore {
    pub fn points(self) -> f64 {
        match self {
            GameScore::Win => 1.0,
            GameScore::Draw => 0.5,
            GameScore::Loss => 0.0,
        }
    }
}

/// One move by the player of interest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnMove {
    /// 0-based ply of the game at which the move was played.
    pub ply: u16,
    /// Position hash before the move.
    pub hash_before: u64,
    /// Position hash after the move.
    pub hash_after: u64,
    pub san: String,
}

/// One game from the perspective of the player of interest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintGame {
    pub color: Color,
    pub score: GameScore,
    pub opponent_elo: Option<u16>,
    pub eco: Option<String>,
    /// The player's own moves within the opening window, plies ascending.
    pub own_moves: Vec<OwnMove>,
}

#[derive(Debug, Serialize)]
pub struct MoveStat {
    pub san: String,
    pub count: u32,
    /// Score percentage for the player over games where this move was
    /// played from this position (0–100, one decimal).
    pub score_pct: f64,
}

/// A position the player reached at least `min_count` times, with the moves
/// chosen from it. Transposition-aware: keyed by position hash regardless
/// of move order.
#[derive(Debug, Serialize)]
pub struct PositionStat {
    pub hash: u64,
    /// Earliest ply at which the position occurred.
    pub min_ply: u16,
    pub count: u32,
    pub moves: Vec<MoveStat>,
}

#[derive(Debug, Serialize)]
pub struct EcoFamilyStat {
    pub eco: String,
    pub games: u32,
    pub score_pct: f64,
}

/// A point where the player left the book: the position was in the theory
/// set but the played move led out of it.
#[derive(Debug, Serialize)]
pub struct DeviationPoint {
    pub hash_before: u64,
    pub san: String,
    /// Earliest ply observed for this deviation.
    pub ply: u16,
    pub count: u32,
    pub score_pct: f64,
}

#[derive(Debug, Serialize)]
pub struct ColorFingerprint {
    pub games: u32,
    pub score_pct: f64,
    pub eco_families: Vec<EcoFamilyStat>,
    pub positions: Vec<PositionStat>,
    pub deviations: Vec<DeviationPoint>,
}

#[derive(Debug, Serialize)]
pub struct RepertoireFingerprint {
    pub player: String,
    pub white: ColorFingerprint,
    pub black: ColorFingerprint,
}

/// Tuning knobs; `Default` is sensible for interactive use.
#[derive(Debug, Clone)]
pub struct FingerprintOptions {
    /// Ignore positions occurring fewer times than this.
    pub min_position_count: u32,
    /// Cap on how many positions/deviations to report per color.
    pub max_rows: usize,
}

impl Default for FingerprintOptions {
    fn default() -> Self {
        Self {
            min_position_count: 2,
            max_rows: 100,
        }
    }
}

fn pct(points: f64, games: u32) -> f64 {
    if games == 0 {
        0.0
    } else {
        (points / games as f64 * 1000.0).round() / 10.0
    }
}

fn color_fingerprint(
    games: &[&FingerprintGame],
    theory: &HashSet<u64>,
    opts: &FingerprintOptions,
) -> ColorFingerprint {
    let total_points: f64 = games.iter().map(|g| g.score.points()).sum();

    // ECO families.
    let mut eco_map: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    for g in games {
        let eco = g.eco.clone().unwrap_or_else(|| "?".into());
        let e = eco_map.entry(eco).or_default();
        e.0 += 1;
        e.1 += g.score.points();
    }
    let mut eco_families: Vec<EcoFamilyStat> = eco_map
        .into_iter()
        .map(|(eco, (n, pts))| EcoFamilyStat {
            eco,
            games: n,
            score_pct: pct(pts, n),
        })
        .collect();
    eco_families.sort_by(|a, b| b.games.cmp(&a.games).then(a.eco.cmp(&b.eco)));

    // Position → move aggregation (transposition-aware by hash).
    #[derive(Default)]
    struct Node {
        count: u32,
        min_ply: u16,
        moves: BTreeMap<String, (u32, f64)>,
    }
    let mut nodes: BTreeMap<u64, Node> = BTreeMap::new();
    // Deviations: (hash_before, san) → (count, points, min_ply).
    let mut devs: BTreeMap<(u64, String), (u32, f64, u16)> = BTreeMap::new();

    for g in games {
        let mut deviated = false;
        for m in &g.own_moves {
            let node = nodes.entry(m.hash_before).or_insert_with(|| Node {
                min_ply: m.ply,
                ..Default::default()
            });
            node.count += 1;
            node.min_ply = node.min_ply.min(m.ply);
            let e = node.moves.entry(m.san.clone()).or_default();
            e.0 += 1;
            e.1 += g.score.points();

            if !deviated && theory.contains(&m.hash_before) && !theory.contains(&m.hash_after) {
                deviated = true; // report only the first exit per game
                let d = devs
                    .entry((m.hash_before, m.san.clone()))
                    .or_insert((0, 0.0, m.ply));
                d.0 += 1;
                d.1 += g.score.points();
                d.2 = d.2.min(m.ply);
            }
        }
    }

    let mut positions: Vec<PositionStat> = nodes
        .into_iter()
        .filter(|(_, n)| n.count >= opts.min_position_count)
        .map(|(hash, n)| {
            let mut moves: Vec<MoveStat> = n
                .moves
                .into_iter()
                .map(|(san, (count, pts))| MoveStat {
                    san,
                    count,
                    score_pct: pct(pts, count),
                })
                .collect();
            moves.sort_by(|a, b| b.count.cmp(&a.count).then(a.san.cmp(&b.san)));
            PositionStat {
                hash,
                min_ply: n.min_ply,
                count: n.count,
                moves,
            }
        })
        .collect();
    positions.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.min_ply.cmp(&b.min_ply))
            .then(a.hash.cmp(&b.hash))
    });
    positions.truncate(opts.max_rows);

    let mut deviations: Vec<DeviationPoint> = devs
        .into_iter()
        .map(|((hash_before, san), (count, pts, ply))| DeviationPoint {
            hash_before,
            san,
            ply,
            count,
            score_pct: pct(pts, count),
        })
        .collect();
    deviations.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.ply.cmp(&b.ply))
            .then(a.san.cmp(&b.san))
    });
    deviations.truncate(opts.max_rows);

    ColorFingerprint {
        games: games.len() as u32,
        score_pct: pct(total_points, games.len() as u32),
        eco_families,
        positions,
        deviations,
    }
}

/// Build the repertoire fingerprint for `player` from their games.
pub fn fingerprint(
    player: &str,
    games: &[FingerprintGame],
    theory: &HashSet<u64>,
    opts: &FingerprintOptions,
) -> RepertoireFingerprint {
    let whites: Vec<&FingerprintGame> = games.iter().filter(|g| g.color == Color::White).collect();
    let blacks: Vec<&FingerprintGame> = games.iter().filter(|g| g.color == Color::Black).collect();
    RepertoireFingerprint {
        player: player.to_string(),
        white: color_fingerprint(&whites, theory, opts),
        black: color_fingerprint(&blacks, theory, opts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own(ply: u16, before: u64, after: u64, san: &str) -> OwnMove {
        OwnMove {
            ply,
            hash_before: before,
            hash_after: after,
            san: san.into(),
        }
    }

    /// Synthetic hashes: 100 = startpos, 101 = after e4, 102 = after e4 c5,
    /// 103 = after e4 e5 (theory); 201, 202 = out-of-book positions.
    fn fixture_games() -> Vec<FingerprintGame> {
        vec![
            // Two Sicilian games as White reaching the same position.
            FingerprintGame {
                color: Color::White,
                score: GameScore::Win,
                opponent_elo: Some(1800),
                eco: Some("B20".into()),
                own_moves: vec![own(0, 100, 101, "e4"), own(2, 102, 201, "Bc4")],
            },
            FingerprintGame {
                color: Color::White,
                score: GameScore::Loss,
                opponent_elo: Some(1900),
                eco: Some("B20".into()),
                own_moves: vec![own(0, 100, 101, "e4"), own(2, 102, 202, "c3")],
            },
            // One open game as White staying in book.
            FingerprintGame {
                color: Color::White,
                score: GameScore::Draw,
                opponent_elo: None,
                eco: Some("C20".into()),
                own_moves: vec![own(0, 100, 101, "e4")],
            },
            // One game as Black.
            FingerprintGame {
                color: Color::Black,
                score: GameScore::Win,
                opponent_elo: Some(1750),
                eco: Some("B20".into()),
                own_moves: vec![own(1, 101, 102, "c5")],
            },
        ]
    }

    #[test]
    fn fingerprint_snapshot() {
        let theory: HashSet<u64> = [100, 101, 102, 103].into_iter().collect();
        let fp = fingerprint(
            "Fixture Player",
            &fixture_games(),
            &theory,
            &FingerprintOptions {
                min_position_count: 1,
                max_rows: 10,
            },
        );
        insta::assert_json_snapshot!(fp);
    }

    #[test]
    fn deviations_are_first_exit_only_and_theory_moves_are_not_deviations() {
        let theory: HashSet<u64> = [100, 101, 102, 103].into_iter().collect();
        let fp = fingerprint(
            "P",
            &fixture_games(),
            &theory,
            &FingerprintOptions::default(),
        );
        // White deviated twice from hash 102 (Bc4 and c3), once per game.
        assert_eq!(fp.white.deviations.len(), 2);
        assert!(fp.white.deviations.iter().all(|d| d.hash_before == 102));
        // e4 keeps the game in book: never a deviation.
        assert!(!fp.white.deviations.iter().any(|d| d.san == "e4"));
        // Black never left book within the window.
        assert!(fp.black.deviations.is_empty());
    }

    #[test]
    fn transpositions_merge_by_hash() {
        let theory = HashSet::new();
        // Same position hash reached in two games at different plies.
        let games = vec![
            FingerprintGame {
                color: Color::White,
                score: GameScore::Win,
                opponent_elo: None,
                eco: None,
                own_moves: vec![own(4, 500, 501, "d4")],
            },
            FingerprintGame {
                color: Color::White,
                score: GameScore::Loss,
                opponent_elo: None,
                eco: None,
                own_moves: vec![own(6, 500, 501, "d4")],
            },
        ];
        let fp = fingerprint("P", &games, &theory, &FingerprintOptions::default());
        assert_eq!(fp.white.positions.len(), 1);
        let p = &fp.white.positions[0];
        assert_eq!((p.count, p.min_ply), (2, 4));
        assert_eq!(p.moves[0].count, 2);
        assert_eq!(p.moves[0].score_pct, 50.0);
    }
}
