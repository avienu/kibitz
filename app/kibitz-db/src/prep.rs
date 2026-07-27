//! Phase 2 prep view: opponent + color → weakest lines → master games
//! reaching those exact positions (docs/ARCHITECTURE.md, Opponent prep).

use kibitz_profile::{Color, ColorFingerprint, RepertoireFingerprint};
use rusqlite::Connection;
use serde::Serialize;

use crate::fingerprint::player_fingerprint;

#[derive(Debug, Serialize)]
pub struct MasterGame {
    pub game_id: i64,
    pub white: String,
    pub black: String,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
    pub event: String,
    pub date: String,
    pub result: String,
    /// Ply at which the game reached the weak-line position.
    pub ply: i64,
}

#[derive(Debug, Serialize)]
pub struct WeakLine {
    /// Position hash of the spot to prepare for.
    pub hash: u64,
    /// ECO code of this spot when it is a book position (bundled dataset),
    /// resolved by position hash — deviations past book have none.
    pub eco: Option<String>,
    /// Opening name for `eco`, from the same dataset entry.
    pub opening_name: Option<String>,
    /// Earliest ply the opponent reached it.
    pub ply: u16,
    /// What the opponent plays there (most frequent first).
    pub opponent_moves: Vec<String>,
    pub games: u32,
    pub score_pct: f64,
    /// Ranking score: frequency-weighted underperformance, earlier is
    /// weightier. Higher = better prep target.
    pub weakness: f64,
    /// True if this spot is also one of the opponent's book-exit points.
    pub deviation: bool,
    pub master_games: Vec<MasterGame>,
}

#[derive(Debug, Clone)]
pub struct PrepOptions {
    pub max_lines: usize,
    pub max_master_games: usize,
    /// Minimum times the opponent must have reached the position.
    pub min_games: u32,
    /// Minimum rating for BOTH players of a "master" game (0 = anyone).
    pub master_min_elo: i64,
}

impl Default for PrepOptions {
    fn default() -> Self {
        Self {
            max_lines: 12,
            max_master_games: 8,
            min_games: 3,
            master_min_elo: 2200,
        }
    }
}

/// Frequency-weighted underperformance, discounted with depth:
/// (50 − score)⁺ · ln(1+games) / (1 + ply/10).
fn weakness_score(score_pct: f64, games: u32, ply: u16) -> f64 {
    let under = (50.0 - score_pct).max(0.0);
    under * ((1 + games) as f64).ln() / (1.0 + ply as f64 / 10.0)
}

/// Build the prep view for an opponent as `color`.
pub fn prep_view(
    conn: &Connection,
    opponent: &str,
    color: Color,
    opts: &PrepOptions,
) -> anyhow::Result<Vec<WeakLine>> {
    let fp: RepertoireFingerprint =
        player_fingerprint(conn, opponent, crate::fingerprint::DEFAULT_MAX_PLIES)?;
    let cf: &ColorFingerprint = match color {
        Color::White => &fp.white,
        Color::Black => &fp.black,
    };
    let deviation_hashes: std::collections::HashSet<u64> =
        cf.deviations.iter().map(|d| d.hash_before).collect();

    let mut lines: Vec<WeakLine> = cf
        .positions
        .iter()
        .filter(|p| p.count >= opts.min_games)
        .map(|p| {
            // Position score = weighted average of its move scores.
            let total: u32 = p.moves.iter().map(|m| m.count).sum();
            let score = if total == 0 {
                50.0
            } else {
                p.moves
                    .iter()
                    .map(|m| m.score_pct * m.count as f64)
                    .sum::<f64>()
                    / total as f64
            };
            WeakLine {
                hash: p.hash,
                eco: None,
                opening_name: None,
                ply: p.min_ply,
                opponent_moves: p.moves.iter().map(|m| m.san.clone()).collect(),
                games: p.count,
                score_pct: (score * 10.0).round() / 10.0,
                weakness: (weakness_score(score, p.count, p.min_ply) * 100.0).round() / 100.0,
                deviation: deviation_hashes.contains(&p.hash),
                master_games: Vec::new(),
            }
        })
        .filter(|l| l.weakness > 0.0)
        .collect();
    // Deviation spots get a ranking nudge: leaving book early is exactly
    // what we want to punish preparation-wise.
    for l in &mut lines {
        if l.deviation {
            l.weakness *= 1.25;
            l.weakness = (l.weakness * 100.0).round() / 100.0;
        }
    }
    lines.sort_by(|a, b| b.weakness.partial_cmp(&a.weakness).unwrap());
    lines.truncate(opts.max_lines);

    // Name the book spots (the openings table is populated: the fingerprint
    // above ran ensure_openings via its theory set).
    for line in &mut lines {
        if let Some((eco, name)) = crate::eco::classify_hash(conn, line.hash)? {
            line.eco = Some(eco);
            line.opening_name = Some(name);
        }
    }

    // Master games reaching each position, best-rated first, then recent.
    let mut stmt = conn.prepare_cached(
        "SELECT g.id, COALESCE(wp.name,'?'), COALESCE(bp.name,'?'),
                g.white_elo, g.black_elo, COALESCE(e.name,'?'),
                COALESCE(g.date,'?'), g.result, MIN(p.ply)
         FROM positions p
         JOIN games g ON g.id = p.game_id
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         LEFT JOIN events e ON e.id = g.event_id
         WHERE p.position_hash = ?1
           AND COALESCE(g.white_elo, 0) >= ?2
           AND COALESCE(g.black_elo, 0) >= ?2
         GROUP BY g.id
         ORDER BY COALESCE(g.white_elo,0) + COALESCE(g.black_elo,0) DESC,
                  g.date DESC
         LIMIT ?3",
    )?;
    for line in &mut lines {
        let rows = stmt.query_map(
            rusqlite::params![
                line.hash as i64,
                opts.master_min_elo,
                opts.max_master_games as i64
            ],
            |r| {
                Ok(MasterGame {
                    game_id: r.get(0)?,
                    white: r.get(1)?,
                    black: r.get(2)?,
                    white_elo: r.get(3)?,
                    black_elo: r.get(4)?,
                    event: r.get(5)?,
                    date: r.get(6)?,
                    result: match r.get::<_, i64>(7)? {
                        1 => "1-0".into(),
                        2 => "0-1".into(),
                        3 => "1/2-1/2".into(),
                        _ => "*".into(),
                    },
                    ply: r.get(8)?,
                })
            },
        )?;
        line.master_games = rows.collect::<Result<_, _>>()?;
    }
    Ok(lines)
}
