//! Read-only database game browser (ROADMAP Phase 1, browse half).
//!
//! IPC commands over silman-db: `open_database`, `list_games`, `get_game`,
//! `opening_tree`, `find_games_at`. One SQLite connection is held in Tauri
//! state (`DbState`); all commands fail cleanly with "no database open"
//! until `open_database` succeeds.
//!
//! Annotation *editing* is deliberately out of scope here (parked decision);
//! everything in this module is read-only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cozy_chess::Board;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::State;

/// The currently open database connection (None until `open_database`).
#[derive(Default)]
pub struct DbState(pub Mutex<Option<Connection>>);

/// How many `find_games_at` hits are serialized to the frontend. The full
/// hit count is still reported in `total` (the start position matches every
/// game in the database, so returning all rows would be pathological).
const FIND_GAMES_MAX_ROWS: usize = 100;

fn result_str(code: i64) -> &'static str {
    match code {
        1 => "1-0",
        2 => "0-1",
        3 => "1/2-1/2",
        _ => "*",
    }
}

fn result_code(s: &str) -> Result<i64, String> {
    match s {
        "1-0" => Ok(1),
        "0-1" => Ok(2),
        "1/2-1/2" => Ok(3),
        "*" => Ok(0),
        other => Err(format!("unknown result filter {other:?}")),
    }
}

/// Run `f` against the open connection, or fail with a clean message.
fn with_conn<T>(
    state: &State<'_, DbState>,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "db state poisoned".to_string())?;
    match guard.as_ref() {
        Some(conn) => f(conn),
        None => Err("no database open (use Open first)".to_string()),
    }
}

/// Resolve `raw` to an existing file. Relative paths are tried against the
/// current directory and each of its ancestors, so the repo-root-relative
/// default `testdata/corpus/scid.sqlite` works from `app/src-tauri` in dev.
fn resolve_db_path(raw: &str) -> Result<PathBuf, String> {
    let p = Path::new(raw);
    if p.is_absolute() {
        return if p.is_file() {
            Ok(p.to_path_buf())
        } else {
            Err(format!("database file not found: {raw}"))
        };
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    for base in cwd.ancestors() {
        let cand = base.join(p);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(format!(
        "database file not found: {raw} (searched {} and its ancestors)",
        cwd.display()
    ))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbSummary {
    pub games: i64,
    pub players: i64,
    pub positions: i64,
    pub sources: i64,
    /// Path actually opened (after relative-path resolution).
    pub path: String,
}

/// Open a silman SQLite database and keep the connection in state.
/// Refuses paths that do not exist (opening would silently create an
/// empty database, which is never what a browser user wants).
#[tauri::command]
pub async fn open_database(state: State<'_, DbState>, path: String) -> Result<DbSummary, String> {
    let resolved = resolve_db_path(&path)?;
    let conn = silman_db::db::open(&resolved).map_err(|e| e.to_string())?;
    let stats = silman_db::query::stats(&conn).map_err(|e| e.to_string())?;
    let summary = DbSummary {
        games: stats.games,
        players: stats.players,
        positions: stats.positions,
        sources: stats.sources,
        path: resolved.display().to_string(),
    };
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "db state poisoned".to_string())?;
    *guard = Some(conn);
    Ok(summary)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFilter {
    /// Case-insensitive substring match on either player's name.
    pub player_substring: Option<String>,
    /// ECO prefix match ("C4" matches C40..C49).
    pub eco: Option<String>,
    /// Exact result: "1-0" | "0-1" | "1/2-1/2" | "*".
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub id: i64,
    pub white: String,
    pub black: String,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
    pub event: String,
    pub date: Option<String>,
    pub result: &'static str,
    pub eco: Option<String>,
    pub ply_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameList {
    pub total: i64,
    pub rows: Vec<GameRow>,
}

const LIST_WHERE: &str = "WHERE (?1 IS NULL
             OR wp.name LIKE '%' || ?1 || '%'
             OR bp.name LIKE '%' || ?1 || '%')
        AND (?2 IS NULL OR g.eco LIKE ?2 || '%')
        AND (?3 IS NULL OR g.result = ?3)";

pub(crate) fn list_games_impl(
    conn: &Connection,
    filter: &GameFilter,
    offset: i64,
    limit: i64,
) -> Result<GameList, String> {
    let player = filter
        .player_substring
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let eco = filter
        .eco
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let result = filter.result.as_deref().map(result_code).transpose()?;
    let limit = limit.clamp(1, 500);
    let offset = offset.max(0);

    let count_sql = format!(
        "SELECT COUNT(*)
         FROM games g
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         {LIST_WHERE}"
    );
    let total: i64 = conn
        .prepare_cached(&count_sql)
        .and_then(|mut stmt| stmt.query_row(rusqlite::params![player, eco, result], |r| r.get(0)))
        .map_err(|e| e.to_string())?;

    let rows_sql = format!(
        "SELECT g.id,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                g.white_elo, g.black_elo,
                COALESCE(e.name, '?'), g.date, g.result, g.eco, g.ply_count
         FROM games g
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         LEFT JOIN events e ON e.id = g.event_id
         {LIST_WHERE}
         ORDER BY g.id DESC
         LIMIT ?4 OFFSET ?5"
    );
    let mut stmt = conn.prepare_cached(&rows_sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params![player, eco, result, limit, offset],
            |row| {
                Ok(GameRow {
                    id: row.get(0)?,
                    white: row.get(1)?,
                    black: row.get(2)?,
                    white_elo: row.get(3)?,
                    black_elo: row.get(4)?,
                    event: row.get(5)?,
                    date: row.get(6)?,
                    result: result_str(row.get(7)?),
                    eco: row.get(8)?,
                    ply_count: row.get(9)?,
                })
            },
        )
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| e.to_string())?;
    Ok(GameList { total, rows })
}

/// List games matching `filter`, newest id first, paged by offset/limit.
#[tauri::command]
pub async fn list_games(
    state: State<'_, DbState>,
    filter: GameFilter,
    offset: i64,
    limit: i64,
) -> Result<GameList, String> {
    with_conn(&state, |conn| list_games_impl(conn, &filter, offset, limit))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub id: i64,
    pub white: String,
    pub black: String,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
    pub event: String,
    pub site: String,
    pub round: Option<String>,
    pub date: Option<String>,
    pub result: &'static str,
    pub eco: Option<String>,
    pub ply_count: i64,
    /// None = standard initial position.
    pub start_fen: Option<String>,
    /// SAN of every mainline ply, decoded from the movetext blob.
    pub sans: Vec<String>,
}

pub(crate) fn get_game_impl(conn: &Connection, id: i64) -> Result<GameDetail, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                    g.white_elo, g.black_elo,
                    COALESCE(e.name, '?'), COALESCE(s.name, '?'),
                    g.round, g.date, g.result, g.eco, g.ply_count,
                    g.movetext, g.start_fen
             FROM games g
             LEFT JOIN players wp ON wp.id = g.white_id
             LEFT JOIN players bp ON bp.id = g.black_id
             LEFT JOIN events e ON e.id = g.event_id
             LEFT JOIN sites s ON s.id = g.site_id
             WHERE g.id = ?1",
        )
        .map_err(|e| e.to_string())?;
    type Row = (
        String,
        String,
        Option<i64>,
        Option<i64>,
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        Option<String>,
        i64,
        Vec<u8>,
        Option<String>,
    );
    let row: Row = stmt
        .query_row([id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
            ))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => format!("no game with id {id}"),
            other => other.to_string(),
        })?;
    let (
        white,
        black,
        white_elo,
        black_elo,
        event,
        site,
        round,
        date,
        result,
        eco,
        ply_count,
        movetext,
        start_fen,
    ) = row;

    let start: Board = match start_fen.as_deref() {
        Some(fen) => fen
            .parse()
            .map_err(|e| format!("game {id} has a bad start FEN {fen:?}: {e:?}"))?,
        None => Board::default(),
    };
    let moves = silman_db::movebin::decode_game(&start, &movetext)
        .map_err(|e| format!("game {id} movetext failed to decode: {e}"))?;
    let mut board = start;
    let mut sans = Vec::with_capacity(moves.len());
    for mv in moves {
        sans.push(silman_db::san::format_san(&board, mv));
        board.play(mv);
    }

    Ok(GameDetail {
        id,
        white,
        black,
        white_elo,
        black_elo,
        event,
        site,
        round,
        date,
        result: result_str(result),
        eco,
        ply_count,
        start_fen,
        sans,
    })
}

/// Full headers + decoded SAN mainline for one game.
#[tauri::command]
pub async fn get_game(state: State<'_, DbState>, id: i64) -> Result<GameDetail, String> {
    with_conn(&state, |conn| get_game_impl(conn, id))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRow {
    pub san: String,
    pub count: i64,
    pub white_wins: i64,
    pub draws: i64,
    pub black_wins: i64,
    pub avg_elo: Option<i64>,
    pub perf: Option<i64>,
}

/// Opening tree for `fen`: every continuation played from this position in
/// the database, with counts, W/D/L (White's perspective), avg mover elo
/// and performance rating.
#[tauri::command]
pub async fn opening_tree(state: State<'_, DbState>, fen: String) -> Result<Vec<TreeRow>, String> {
    with_conn(&state, |conn| {
        let (moves, _elapsed) =
            silman_db::query::opening_tree(conn, &fen).map_err(|e| e.to_string())?;
        Ok(moves
            .into_iter()
            .map(|m| TreeRow {
                san: m.san,
                count: m.count,
                white_wins: m.white_wins,
                draws: m.draws,
                black_wins: m.black_wins,
                avg_elo: m.avg_elo,
                perf: m.perf,
            })
            .collect())
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAtRow {
    pub id: i64,
    pub white: String,
    pub black: String,
    pub event: String,
    pub date: String,
    pub result: &'static str,
    /// First ply at which the position occurred (1-based).
    pub ply: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesAt {
    /// Total number of games that reached the position.
    pub total: i64,
    /// At most [`FIND_GAMES_MAX_ROWS`] hits (ordered by game id).
    pub rows: Vec<GameAtRow>,
}

/// Games whose mainline reached the position `fen` (position-hash lookup).
#[tauri::command]
pub async fn find_games_at(state: State<'_, DbState>, fen: String) -> Result<GamesAt, String> {
    with_conn(&state, |conn| {
        let (hits, _elapsed) = silman_db::query::find_fen(conn, &fen).map_err(|e| e.to_string())?;
        let total = hits.len() as i64;
        let rows = hits
            .into_iter()
            .take(FIND_GAMES_MAX_ROWS)
            .map(|h| GameAtRow {
                id: h.game_id,
                white: h.white,
                black: h.black,
                event: h.event,
                date: h.date,
                result: h.result,
                ply: h.ply,
            })
            .collect();
        Ok(GamesAt { total, rows })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use silman_db::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// Opera game (public domain), a fool's-mate miniature with elos, and a
    /// short unfinished game — enough to exercise every list filter.
    const FIXTURE: &str = r#"[Event "Casual Game"]
[Site "Paris FRA"]
[Date "1858.11.02"]
[White "Morphy, Paul"]
[Black "Duke Karl / Count Isouard"]
[Result "1-0"]

1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7
8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7
14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0

[Event "Test Miniature"]
[White "Someone"]
[Black "Someone Else"]
[Result "0-1"]
[WhiteElo "2200"]
[BlackElo "2300"]

1. f3 e5 2. g4 Qh4# 0-1

[Event "Unfinished"]
[White "Alpha"]
[Black "Beta"]
[Result "*"]

1. d4 d5 *
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = silman_db::db::open(&dir.path().join("test.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 3, "failures: {:?}", st.failures);
        (dir, conn)
    }

    #[test]
    fn list_games_pages_and_orders_newest_first() {
        let (_dir, conn) = fixture_db();
        let all = list_games_impl(&conn, &GameFilter::default(), 0, 50).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.rows.len(), 3);
        // ORDER BY id DESC: last imported first.
        assert_eq!(all.rows[0].id, 3);
        assert_eq!(all.rows[0].white, "Alpha");
        assert_eq!(all.rows[0].result, "*");
        assert_eq!(all.rows[2].id, 1);
        assert_eq!(all.rows[2].white, "Morphy, Paul");
        assert_eq!(all.rows[2].ply_count, 33);

        let page2 = list_games_impl(&conn, &GameFilter::default(), 1, 1).unwrap();
        assert_eq!(page2.total, 3, "total ignores paging");
        assert_eq!(page2.rows.len(), 1);
        assert_eq!(page2.rows[0].id, 2);
    }

    #[test]
    fn list_games_filters_by_player_result_and_eco() {
        let (_dir, conn) = fixture_db();

        let filter = GameFilter {
            player_substring: Some("morphy".into()),
            ..Default::default()
        };
        let hits = list_games_impl(&conn, &filter, 0, 50).unwrap();
        assert_eq!(hits.total, 1, "LIKE is case-insensitive for ASCII");
        assert_eq!(hits.rows[0].white, "Morphy, Paul");

        // Substring matches the black side too.
        let filter = GameFilter {
            player_substring: Some("Isouard".into()),
            ..Default::default()
        };
        assert_eq!(list_games_impl(&conn, &filter, 0, 50).unwrap().total, 1);

        let filter = GameFilter {
            result: Some("0-1".into()),
            ..Default::default()
        };
        let hits = list_games_impl(&conn, &filter, 0, 50).unwrap();
        assert_eq!(hits.total, 1);
        assert_eq!(hits.rows[0].white_elo, Some(2200));
        assert_eq!(hits.rows[0].black_elo, Some(2300));

        // The Morphy game is ECO-tagged C41 at import; prefix filter.
        let filter = GameFilter {
            eco: Some("C4".into()),
            ..Default::default()
        };
        let hits = list_games_impl(&conn, &filter, 0, 50).unwrap();
        assert_eq!(hits.total, 1);
        assert_eq!(hits.rows[0].id, 1);

        let filter = GameFilter {
            result: Some("2-0".into()),
            ..Default::default()
        };
        assert!(list_games_impl(&conn, &filter, 0, 50).is_err());
    }

    #[test]
    fn get_game_decodes_the_movetext_back_to_san() {
        let (_dir, conn) = fixture_db();
        let g = get_game_impl(&conn, 1).unwrap();
        assert_eq!(g.white, "Morphy, Paul");
        assert_eq!(g.black, "Duke Karl / Count Isouard");
        assert_eq!(g.result, "1-0");
        assert_eq!(g.date.as_deref(), Some("1858.11.02"));
        assert_eq!(g.start_fen, None);
        assert_eq!(g.ply_count, 33);
        assert_eq!(g.sans.len(), 33);
        assert_eq!(g.sans[0], "e4");
        assert_eq!(g.sans[20], "Bxb5+");
        assert_eq!(g.sans[32], "Rd8#");

        let mini = get_game_impl(&conn, 2).unwrap();
        assert_eq!(mini.sans, vec!["f3", "e5", "g4", "Qh4#"]);

        assert!(get_game_impl(&conn, 999).unwrap_err().contains("999"));
    }
}
