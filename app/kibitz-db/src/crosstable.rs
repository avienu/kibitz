//! Event crosstable query (run 10): every game of a named event, with the
//! header fields a crosstable needs (players, Elos, round, result, date).
//!
//! This module only QUERIES; the grid itself (round parsing, the players ×
//! rounds layout, the Swiss degrade to a scored list) is presentation
//! logic and lives in the frontend (app/src/lib/crosstable.ts), where it
//! is unit-tested against the same fixtures.

use rusqlite::Connection;

/// One game of the event, in import order.
#[derive(Debug, Clone)]
pub struct EventGame {
    pub game_id: i64,
    pub white: String,
    pub black: String,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
    /// Raw PGN Round tag: "1", "1.2", "?", or absent.
    pub round: Option<String>,
    pub result: &'static str,
    pub date: Option<String>,
}

/// Cap on the games serialized for one event — a mistagged mega-"event"
/// (every TWIC game filed under "?") must not ship half the database to
/// the webview. The true count is still reported.
pub const EVENT_GAMES_MAX: usize = 1000;

fn result_str(code: i64) -> &'static str {
    match code {
        1 => "1-0",
        2 => "0-1",
        3 => "1/2-1/2",
        _ => "*",
    }
}

/// All games whose event name is exactly `event` (first
/// [`EVENT_GAMES_MAX`] rows), plus the true total.
pub fn event_games(conn: &Connection, event: &str) -> rusqlite::Result<(Vec<EventGame>, i64)> {
    let total: i64 = conn
        .prepare_cached(
            "SELECT COUNT(*) FROM games g JOIN events e ON e.id = g.event_id WHERE e.name = ?1",
        )?
        .query_row([event], |r| r.get(0))?;
    let mut stmt = conn.prepare_cached(
        "SELECT g.id,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                g.white_elo, g.black_elo, g.round, g.result, g.date
         FROM games g
         JOIN events e ON e.id = g.event_id
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         WHERE e.name = ?1
         ORDER BY g.id
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![event, EVENT_GAMES_MAX as i64], |row| {
            Ok(EventGame {
                game_id: row.get(0)?,
                white: row.get(1)?,
                black: row.get(2)?,
                white_elo: row.get(3)?,
                black_elo: row.get(4)?,
                round: row.get(5)?,
                result: result_str(row.get(6)?),
                date: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((rows, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// A 3-player single round robin with full Round tags, and a partial-
    /// rounds event mixing sub-rounds ("1.2"), "?" and a missing tag.
    const FIXTURE: &str = r#"[Event "Mini RR"]
[Round "1"]
[White "Alpha"]
[Black "Bravo"]
[WhiteElo "2400"]
[BlackElo "2300"]
[Result "1-0"]

1. e4 e5 1-0

[Event "Mini RR"]
[Round "2"]
[White "Bravo"]
[Black "Charlie"]
[WhiteElo "2300"]
[BlackElo "2200"]
[Result "1/2-1/2"]

1. d4 d5 1/2-1/2

[Event "Mini RR"]
[Round "3"]
[White "Charlie"]
[Black "Alpha"]
[WhiteElo "2200"]
[BlackElo "2400"]
[Result "0-1"]

1. c4 c5 0-1

[Event "Ragged Swiss"]
[Round "1.2"]
[White "Delta"]
[Black "Echo"]
[Result "1-0"]

1. g3 g6 1-0

[Event "Ragged Swiss"]
[Round "?"]
[White "Echo"]
[Black "Foxtrot"]
[Result "0-1"]

1. b3 b6 0-1

[Event "Ragged Swiss"]
[White "Foxtrot"]
[Black "Delta"]
[Result "*"]

1. a3 *
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 6, "failures: {:?}", st.failures);
        (dir, conn)
    }

    #[test]
    fn round_robin_event_returns_every_game_with_rounds_and_elos() {
        let (_dir, conn) = fixture_db();
        let (games, total) = event_games(&conn, "Mini RR").unwrap();
        assert_eq!(total, 3);
        assert_eq!(games.len(), 3);
        assert_eq!(games[0].white, "Alpha");
        assert_eq!(games[0].black, "Bravo");
        assert_eq!(games[0].round.as_deref(), Some("1"));
        assert_eq!(games[0].white_elo, Some(2400));
        assert_eq!(games[0].result, "1-0");
        assert_eq!(games[2].round.as_deref(), Some("3"));
        assert_eq!(games[2].result, "0-1");
    }

    #[test]
    fn partial_rounds_event_keeps_raw_round_tags_including_missing_ones() {
        let (_dir, conn) = fixture_db();
        let (games, total) = event_games(&conn, "Ragged Swiss").unwrap();
        assert_eq!(total, 3);
        let rounds: Vec<Option<&str>> = games.iter().map(|g| g.round.as_deref()).collect();
        // Raw tags pass through untouched — the frontend's tolerant round
        // parser owns "1.2" / "?" / missing (never a crash here).
        assert_eq!(rounds, vec![Some("1.2"), Some("?"), None]);
        assert_eq!(games[2].result, "*");
        assert_eq!(games[2].white_elo, None);
    }

    #[test]
    fn unknown_event_is_empty_not_an_error() {
        let (_dir, conn) = fixture_db();
        let (games, total) = event_games(&conn, "No Such Open").unwrap();
        assert!(games.is_empty());
        assert_eq!(total, 0);
    }
}
