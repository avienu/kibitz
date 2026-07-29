//! Adapter: feed database games into kibitz-profile's repertoire
//! fingerprint (which is pure and knows nothing about SQLite).

use std::collections::HashSet;

use cozy_chess::{Board, Color as CozyColor};
use kibitz_profile::{
    fingerprint, Color, FingerprintGame, FingerprintOptions, GameScore, OwnMove,
    RepertoireFingerprint,
};
use rusqlite::{Connection, OptionalExtension};

use crate::hash::position_hash;
use crate::movebin::decode_game;
use crate::san::format_san;

/// Opening window: only the player's moves within the first this-many plies
/// of a game contribute to the repertoire fingerprint.
pub const DEFAULT_MAX_PLIES: u16 = 40;

/// Names in the players table matching `pattern` (for CLI suggestions).
pub fn matching_players(conn: &Connection, pattern: &str) -> rusqlite::Result<Vec<String>> {
    // Suggestions are grouped by IDENTITY (2026-07-29 field report: the
    // dropdown listed "O'Connor, Shawn" and "Shawn O'Connor" side by
    // side, which reads as "the app thinks these are two people" even
    // though profile/prep resolve them together). Same grouping rule as
    // identity::resolve_identity — declared alias group first, else the
    // lexical key — with the games-heaviest form as the representative;
    // selecting it resolves the whole identity downstream anyway.
    let mut stmt = conn.prepare(
        "SELECT id, name FROM players WHERE name LIKE '%' || ?1 || '%' ORDER BY name LIMIT 40",
    )?;
    let raw: Vec<(i64, String)> = stmt
        .query_map([pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    // Two candidates belong to one identity when they share a lexical
    // key OR a declared group — and those relations chain (a declared
    // member's lexical twins join too, mirroring resolve_identity's
    // transitive closure). Tiny union-find over the candidate set.
    use std::collections::HashMap;
    let n = raw.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut games_of: Vec<i64> = Vec::with_capacity(n);
    for (i, (id, name)) in raw.iter().enumerate() {
        games_of.push(conn.query_row(
            "SELECT (SELECT COUNT(*) FROM games WHERE white_id = ?1)
                  + (SELECT COUNT(*) FROM games WHERE black_id = ?1)",
            [id],
            |r| r.get(0),
        )?);
        let declared: Option<i64> = conn
            .query_row(
                "SELECT group_id FROM alias_members WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .optional()?;
        let mut keys = vec![format!("k{}", crate::identity::identity_key(name))];
        if let Some(g) = declared {
            keys.push(format!("g{g}"));
        }
        for key in keys {
            match by_key.get(&key) {
                Some(&j) => {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                }
                None => {
                    by_key.insert(key, i);
                }
            }
        }
    }
    // Representative per component: the games-heaviest form.
    let mut best: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let r = root(&mut parent, i);
        let cur = best.entry(r).or_insert(i);
        if games_of[i] > games_of[*cur] {
            *cur = i;
        }
    }
    let mut out: Vec<String> = best.values().map(|&i| raw[i].1.clone()).collect();
    out.sort();
    out.truncate(20);
    Ok(out)
}

/// The set of book-position hashes from the bundled openings dataset.
pub fn theory_set(conn: &Connection) -> anyhow::Result<HashSet<u64>> {
    crate::eco::ensure_openings(conn)?;
    let mut stmt = conn.prepare("SELECT DISTINCT position_hash FROM openings")?;
    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    Ok(rows
        .map(|r| r.map(|h| h as u64))
        .collect::<Result<_, _>>()?)
}

/// Build the repertoire fingerprint for the player with this exact name.
pub fn player_fingerprint(
    conn: &Connection,
    player: &str,
    max_plies: u16,
) -> anyhow::Result<RepertoireFingerprint> {
    // Identity-resolved (run 8.5): lexical name variants + declared
    // aliases fingerprint as one player.
    let ids = crate::identity::resolve_identity_ids(conn, player)?;
    if ids.is_empty() {
        anyhow::bail!(
            "no player named {player:?} (try `kibitz-cli players {player:?}` for matches)"
        );
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");

    let theory = theory_set(conn)?;

    let mut stmt = conn.prepare(&format!(
        "SELECT white_id IN ({id_list}), result, white_elo, black_elo, eco, movetext, start_fen
         FROM games
         WHERE white_id IN ({id_list}) OR black_id IN ({id_list})"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, bool>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Vec<u8>>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut games = Vec::new();
    for row in rows {
        let (is_white, result, white_elo, black_elo, eco, movetext, start_fen) = row?;
        // Only decided/drawn standard-start games contribute; custom-start
        // fragments are studies, not repertoire evidence.
        if start_fen.is_some() {
            continue;
        }
        let score = match (result, is_white) {
            (1, true) | (2, false) => GameScore::Win,
            (2, true) | (1, false) => GameScore::Loss,
            (3, _) => GameScore::Draw,
            _ => continue, // unfinished
        };
        let start = Board::default();
        let Ok(moves) = decode_game(&start, &movetext) else {
            continue;
        };
        let my_color = if is_white {
            CozyColor::White
        } else {
            CozyColor::Black
        };
        let mut board = start;
        let mut own_moves = Vec::new();
        for (ply, mv) in moves.iter().enumerate().take(max_plies as usize) {
            let hash_before = position_hash(&board);
            let is_mine = board.side_to_move() == my_color;
            let san = if is_mine {
                Some(format_san(&board, *mv))
            } else {
                None
            };
            board.play(*mv);
            if let Some(san) = san {
                own_moves.push(OwnMove {
                    ply: ply as u16,
                    hash_before,
                    hash_after: position_hash(&board),
                    san,
                });
            }
        }
        games.push(FingerprintGame {
            color: if is_white { Color::White } else { Color::Black },
            score,
            opponent_elo: if is_white { black_elo } else { white_elo }.map(|e| e as u16),
            eco,
            own_moves,
        });
    }

    Ok(fingerprint(
        player,
        &games,
        &theory,
        &FingerprintOptions::default(),
    ))
}

/// An example game that reached `hash` (for CLI context on deviations).
pub fn example_game_at(conn: &Connection, hash: u64) -> Option<(i64, String, String)> {
    conn.query_row(
        "SELECT g.id, COALESCE(wp.name,'?'), COALESCE(bp.name,'?')
         FROM positions p JOIN games g ON g.id = p.game_id
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         WHERE p.position_hash = ?1 LIMIT 1",
        [hash as i64],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .ok()
}
