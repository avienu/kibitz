//! App-side PlayerProfile pipeline: extract per-ply data from stored games
//! (static analysis + any engine evals in `analyses`) and feed the pure
//! aggregator in kibitz-profile.

use cozy_chess::{Board, Color as CozyColor};
use kibitz_core::record::{Magnitude, Phase, Severity};
use kibitz_profile::{
    player_profile, Color, GameScore, PhaseTag, PlayerProfile, ProfileGame, ProfilePly,
};
use rusqlite::{params, Connection};

/// Evals for one game keyed by mainline ply, White's point of view.
/// Fresh rows (with their engine identity) are preferred over legacy
/// imports at the same ply; legacy evals are already White-POV (SCID
/// convention), fresh rows are side-to-move-POV and get converted here.
fn game_evals(
    conn: &Connection,
    game_id: i64,
) -> anyhow::Result<std::collections::HashMap<u16, i32>> {
    let mut stmt = conn.prepare_cached(
        "SELECT ply, kind, eval_cp FROM analyses WHERE game_id = ?1 ORDER BY
         ply, CASE kind WHEN 'fresh' THEN 0 ELSE 1 END, id DESC",
    )?;
    let rows = stmt.query_map([game_id], |r| {
        Ok((
            r.get::<_, i64>(0)? as u16,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)? as i32,
        ))
    })?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (ply, kind, cp) = row?;
        map.entry(ply).or_insert_with(|| {
            if kind == "fresh" {
                // Fresh evals are from the side to move at that position:
                // after `ply` plies, White moves iff ply is even.
                if ply % 2 == 0 {
                    cp
                } else {
                    -cp
                }
            } else {
                cp // legacy imports are White-POV
            }
        });
    }
    Ok(map)
}

fn phase_tag(p: Phase) -> PhaseTag {
    match p {
        Phase::Opening => PhaseTag::Opening,
        Phase::Middlegame => PhaseTag::Middlegame,
        Phase::Endgame => PhaseTag::Endgame,
    }
}

/// Build the full profile for `player` over up to `max_games` most recent
/// standard-start games.
pub fn build_profile(
    conn: &Connection,
    player: &str,
    max_games: u32,
) -> anyhow::Result<PlayerProfile> {
    // Identity resolution (run 8.5): the same person under different name
    // forms ("O'Connor, Shawn" / "Shawn O'Connor") and declared aliases
    // (online handles) profiles as ONE player.
    let ids = crate::identity::resolve_identity_ids(conn, player)?;
    if ids.is_empty() {
        anyhow::bail!("no player named {player:?}");
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");

    let mut stmt = conn.prepare(&format!(
        "SELECT id, white_id IN ({id_list}), result, eco, movetext
         FROM games
         WHERE (white_id IN ({id_list}) OR black_id IN ({id_list}))
           AND start_fen IS NULL
         ORDER BY id DESC LIMIT ?1"
    ))?;
    type GameRow = (i64, bool, i64, Option<String>, Vec<u8>);
    let rows: Vec<GameRow> = stmt
        .query_map(params![max_games as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut games = Vec::new();
    for (game_id, is_white, result, eco, movetext) in rows {
        let score = match (result, is_white) {
            (1, true) | (2, false) => GameScore::Win,
            (2, true) | (1, false) => GameScore::Loss,
            (3, _) => GameScore::Draw,
            _ => continue,
        };
        let start = Board::default();
        let Ok(moves) = crate::movebin::decode_game(&start, &movetext) else {
            continue;
        };
        if moves.len() < 10 {
            continue;
        }
        let subject = if is_white {
            CozyColor::White
        } else {
            CozyColor::Black
        };
        let evals = game_evals(conn, game_id)?;
        let subject_pov = |cp: i32| if subject == CozyColor::White { cp } else { -cp };

        // Analyze every mainline position once; consecutive pairs give
        // each move its before/after alert sets.
        let mut board = start.clone();
        #[allow(clippy::type_complexity)]
        let mut per_pos: Vec<(Vec<(String, bool)>, Phase)> = Vec::with_capacity(moves.len() + 1);
        let alerts_of = |b: &Board| -> Vec<(String, bool)> {
            kibitz_core::wsui::screen(b, &kibitz_core::wsui::WsuiConfig::default())
                .alerts
                .into_iter()
                .filter(|a| a.severity >= Severity::Medium)
                .map(|a| {
                    let against_subject = match a.side {
                        kibitz_core::record::SideColor::White => subject == CozyColor::White,
                        kibitz_core::record::SideColor::Black => subject == CozyColor::Black,
                    };
                    (format!("{:?}", a.kind), against_subject)
                })
                .collect()
        };
        per_pos.push((alerts_of(&board), kibitz_core::imbalance::phase(&board)));
        for &mv in &moves {
            board.play(mv);
            per_pos.push((alerts_of(&board), kibitz_core::imbalance::phase(&board)));
        }

        // Structure flags from a mid-game sample position.
        let sample_ply = (moves.len() * 2 / 3).clamp(1, moves.len());
        let mut sb = start.clone();
        for &mv in &moves[..sample_ply] {
            sb.play(mv);
        }
        let mut structure_flags = Vec::new();
        for imb in kibitz_core::imbalance::assess(&sb) {
            if imb.magnitude < Magnitude::Minor {
                continue;
            }
            let me = if subject == CozyColor::White {
                "white"
            } else {
                "black"
            };
            let opp = if subject == CozyColor::White {
                "black"
            } else {
                "white"
            };
            for key in imb.evidence.keys() {
                let flag = if key == &format!("isolated_{me}") {
                    Some("own-isolated-pawn")
                } else if key == &format!("backward_{me}") {
                    Some("own-backward-pawn")
                } else if key == &format!("doubled_{me}") {
                    Some("own-doubled-pawns")
                } else if key == &format!("passed_{me}") {
                    Some("own-passed-pawn")
                } else if key == &format!("bad_bishop_{me}") {
                    Some("own-bad-bishop")
                } else if key == &format!("bad_bishop_{opp}") {
                    Some("opp-bad-bishop")
                } else if key == &format!("holes_in_{me}_camp") {
                    Some("holes-in-own-camp")
                } else {
                    None
                };
                if let Some(f) = flag {
                    if !structure_flags.contains(&f.to_string()) {
                        structure_flags.push(f.to_string());
                    }
                }
            }
        }

        let mut plies = Vec::with_capacity(moves.len());
        for i in 1..=moves.len() {
            let mover_is_white = i % 2 == 1;
            let subject_moved = mover_is_white == (subject == CozyColor::White);
            plies.push(ProfilePly {
                ply: i as u16,
                subject_moved,
                phase: phase_tag(per_pos[i].1),
                alerts_before: per_pos[i - 1].0.clone(),
                alerts_after: per_pos[i].0.clone(),
                eval_before: evals.get(&((i - 1) as u16)).map(|&c| subject_pov(c)),
                eval_after: evals.get(&(i as u16)).map(|&c| subject_pov(c)),
            });
        }

        games.push(ProfileGame {
            game_id,
            color: if is_white { Color::White } else { Color::Black },
            score,
            eco,
            structure_flags,
            structure_ply: sample_ply as u16,
            plies,
        });
    }

    Ok(player_profile(player, &games))
}
