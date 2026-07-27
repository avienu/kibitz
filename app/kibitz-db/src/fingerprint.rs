//! Adapter: feed database games into kibitz-profile's repertoire
//! fingerprint (which is pure and knows nothing about SQLite).

use std::collections::HashSet;

use cozy_chess::{Board, Color as CozyColor};
use kibitz_profile::{
    fingerprint, Color, FingerprintGame, FingerprintOptions, GameScore, OwnMove,
    RepertoireFingerprint,
};
use rusqlite::Connection;

use crate::hash::position_hash;
use crate::movebin::decode_game;
use crate::san::format_san;

/// Opening window: only the player's moves within the first this-many plies
/// of a game contribute to the repertoire fingerprint.
pub const DEFAULT_MAX_PLIES: u16 = 40;

/// Names in the players table matching `pattern` (for CLI suggestions).
pub fn matching_players(conn: &Connection, pattern: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM players WHERE name LIKE '%' || ?1 || '%' ORDER BY name LIMIT 20",
    )?;
    let rows = stmt.query_map([pattern], |r| r.get(0))?;
    rows.collect()
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
