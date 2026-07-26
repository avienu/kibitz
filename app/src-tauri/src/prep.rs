//! Opponent-prep IPC commands (ROADMAP Phase 2, prep view).
//!
//! `matching_players` wraps silman_db::fingerprint::matching_players for
//! the opponent-name suggestions; `prep_view` wraps silman_db::prep::
//! prep_view with default options. Read-only over the open database; the
//! engine is never involved (CLAUDE.md #6).

use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterGameDto {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakLineDto {
    /// Position hash as a decimal string (u64 does not fit a JS number).
    pub hash: String,
    /// Earliest ply the opponent reached the position.
    pub ply: u16,
    /// What the opponent plays there (most frequent first).
    pub opponent_moves: Vec<String>,
    pub games: u32,
    pub score_pct: f64,
    pub weakness: f64,
    /// True if this spot is also one of the opponent's book-exit points.
    pub deviation: bool,
    pub master_games: Vec<MasterGameDto>,
}

fn parse_color(color: &str) -> Result<silman_profile::Color, String> {
    match color {
        "white" => Ok(silman_profile::Color::White),
        "black" => Ok(silman_profile::Color::Black),
        other => Err(format!(
            "color must be \"white\" or \"black\", got {other:?}"
        )),
    }
}

pub(crate) fn prep_view_impl(
    conn: &rusqlite::Connection,
    player: &str,
    color: &str,
) -> Result<Vec<WeakLineDto>, String> {
    let color = parse_color(color)?;
    let lines = silman_db::prep::prep_view(
        conn,
        player,
        color,
        &silman_db::prep::PrepOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(lines
        .into_iter()
        .map(|l| WeakLineDto {
            hash: l.hash.to_string(),
            ply: l.ply,
            opponent_moves: l.opponent_moves,
            games: l.games,
            score_pct: l.score_pct,
            weakness: l.weakness,
            deviation: l.deviation,
            master_games: l
                .master_games
                .into_iter()
                .map(|m| MasterGameDto {
                    game_id: m.game_id,
                    white: m.white,
                    black: m.black,
                    white_elo: m.white_elo,
                    black_elo: m.black_elo,
                    event: m.event,
                    date: m.date,
                    result: m.result,
                    ply: m.ply,
                })
                .collect(),
        })
        .collect())
}

/// Player names matching `pattern` (substring, for opponent suggestions).
#[tauri::command]
pub async fn matching_players(
    state: State<'_, DbState>,
    pattern: String,
) -> Result<Vec<String>, String> {
    with_conn(&state, |conn| {
        silman_db::fingerprint::matching_players(conn, &pattern).map_err(|e| e.to_string())
    })
}

/// Build the prep view for an opponent as `color` ("white" | "black").
#[tauri::command]
pub async fn prep_view(
    state: State<'_, DbState>,
    player: String,
    color: String,
) -> Result<Vec<WeakLineDto>, String> {
    with_conn(&state, |conn| prep_view_impl(conn, &player, &color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use silman_db::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// Villain scores 0.5/4 in the Scandinavian as Black; one 2600+ master
    /// game shares the line, one 1500-rated game must be excluded
    /// (mirrors silman-db's own hand-computed prep fixture).
    const CORPUS: &str = r#"[White "Hero"]
[Black "Villain"]
[Date "2024.01.01"]
[Result "1-0"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 c6 5. Nf3 Nf6 1-0

[White "Hero"]
[Black "Villain"]
[Date "2024.01.08"]
[Result "1-0"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 5. Nf3 c6 1-0

[White "SomeoneElse"]
[Black "Villain"]
[Date "2024.02.01"]
[Result "1-0"]

1. e4 d5 2. exd5 Nf6 3. d4 Nxd5 4. Nf3 g6 5. Be2 Bg7 1-0

[White "Fourth"]
[Black "Villain"]
[Date "2024.03.01"]
[Result "1/2-1/2"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qd6 4. d4 Nf6 5. Nf3 a6 1/2-1/2

[Event "Masters"]
[White "GM Alpha"]
[Black "GM Beta"]
[Date "2023.05.01"]
[Result "1-0"]
[WhiteElo "2650"]
[BlackElo "2600"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 5. Nf3 Bf5 6. Bc4 e6 1-0

[Event "Club"]
[White "Patzer One"]
[Black "Patzer Two"]
[Date "2023.06.01"]
[Result "0-1"]
[WhiteElo "1500"]
[BlackElo "1450"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 0-1
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "test".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(CORPUS)).unwrap();
        assert_eq!(st.games_imported, 6, "failures: {:?}", st.failures);
        (dir, conn)
    }

    #[test]
    fn prep_view_command_smoke() {
        let (_dir, conn) = fixture_db();
        let lines = prep_view_impl(&conn, "Villain", "black").unwrap();
        assert!(!lines.is_empty(), "must find weak lines");
        let top = &lines[0];
        assert_eq!(top.games, 4);
        assert!((top.score_pct - 12.5).abs() < 0.01, "{}", top.score_pct);
        assert!(top.ply <= 2);

        // Master game offered somewhere; the club game never.
        let masters: Vec<&str> = lines
            .iter()
            .flat_map(|l| l.master_games.iter().map(|m| m.white.as_str()))
            .collect();
        assert!(masters.contains(&"GM Alpha"), "{masters:?}");
        assert!(!masters.contains(&"Patzer One"), "{masters:?}");

        // Wire shape: camelCase keys, hash as string.
        let json = serde_json::to_string(&lines[0]).unwrap();
        for needle in [
            "\"scorePct\":",
            "\"opponentMoves\":",
            "\"masterGames\":",
            "\"hash\":\"",
        ] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }

        // Bad inputs fail cleanly.
        assert!(prep_view_impl(&conn, "Villain", "purple").is_err());
        assert!(prep_view_impl(&conn, "Nobody Such", "white").is_err());
    }

    #[test]
    fn matching_players_suggests_substrings() {
        let (_dir, conn) = fixture_db();
        let names = silman_db::fingerprint::matching_players(&conn, "illa").unwrap();
        assert_eq!(names, vec!["Villain".to_string()]);
    }
}
