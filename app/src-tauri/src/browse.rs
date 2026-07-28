//! Read-only database game browser (ROADMAP Phase 1, browse half).
//!
//! IPC commands over kibitz-db: `open_database`, `list_games`, `get_game`,
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

/// True when an error string is SQLite's transient contention signal
/// (SQLITE_BUSY "database is locked" / SQLITE_LOCKED "database table is
/// locked"). String-level detection because every command maps
/// `rusqlite::Error` to `String` at the boundary; the real-contention
/// test below pins the match against rusqlite's actual message text.
pub(crate) fn is_busy_msg(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("database is locked") || m.contains("database table is locked")
}

/// Backoff schedule for [`retry_busy`] (milliseconds between attempts).
const BUSY_RETRY_DELAYS_MS: [u64; 3] = [50, 100, 250];

/// Retry `f` when it fails with a busy/locked error (audit #2/#7): while
/// a background import worker (TWIC, account syncs, jobs) commits a write
/// transaction on its own connection, a read or meta write on the shared
/// UI connection can surface SQLITE_BUSY once its `busy_timeout` elapses.
/// Those errors are transient — retry a few times with short sleeps
/// instead of failing the command. Real errors pass through untouched on
/// the first attempt. `sleep` is injectable so tests assert the schedule
/// without waiting.
pub(crate) fn retry_busy<T>(
    mut f: impl FnMut() -> Result<T, String>,
    sleep: &mut dyn FnMut(std::time::Duration),
) -> Result<T, String> {
    let mut attempt = 0usize;
    loop {
        match f() {
            Err(e) if is_busy_msg(&e) && attempt < BUSY_RETRY_DELAYS_MS.len() => {
                sleep(std::time::Duration::from_millis(
                    BUSY_RETRY_DELAYS_MS[attempt],
                ));
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// Run `f` against the open connection, or fail with a clean message.
/// Busy/locked failures are retried per [`retry_busy`] — a bulk import on
/// a worker connection must never surface as a spurious command error.
pub(crate) fn with_conn<T>(
    state: &State<'_, DbState>,
    mut f: impl FnMut(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "db state poisoned".to_string())?;
    match guard.as_ref() {
        Some(conn) => retry_busy(|| f(conn), &mut std::thread::sleep),
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

/// Open a kibitz SQLite database and keep the connection in state.
/// Refuses paths that do not exist (opening would silently create an
/// empty database, which is never what a browser user wants).
#[tauri::command]
pub async fn open_database(state: State<'_, DbState>, path: String) -> Result<DbSummary, String> {
    let resolved = resolve_db_path(&path)?;
    let conn = kibitz_db::db::open(&resolved).map_err(|e| e.to_string())?;
    // Wait out write locks held by worker connections (run_jobs, TWIC /
    // account syncs) for up to 5 s per statement. Locks held longer than
    // that still error — with_conn's retry_busy layer absorbs those.
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let summary = db_summary_impl(&conn)?;
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "db state poisoned".to_string())?;
    *guard = Some(conn);
    Ok(summary)
}

pub(crate) fn db_summary_impl(conn: &Connection) -> Result<DbSummary, String> {
    let stats = kibitz_db::query::stats(conn).map_err(|e| e.to_string())?;
    let path: String = conn
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(DbSummary {
        games: stats.games,
        players: stats.players,
        positions: stats.positions,
        sources: stats.sources,
        path,
    })
}

/// Fresh summary counts for the open database — the SINGLE source every
/// game-count display consumes (rail badge, Database header, list-total
/// refetch trigger). The frontend re-polls this on one cadence during
/// network syncs so all counts move together (audit #8).
#[tauri::command]
pub async fn db_summary(state: State<'_, DbState>) -> Result<DbSummary, String> {
    with_conn(&state, db_summary_impl)
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
    /// Source name (e.g. "TWIC 1594") — the Database table's SOURCE tag.
    pub source: String,
    /// Source kind for tag colouring: personal | twic | online | other.
    pub source_kind: String,
    /// True when duplicate copies are linked to this game (the ⑂ flag —
    /// duplicates are linked to their higher-priority copy, never deleted).
    pub dup: bool,
    /// Analysis presence per the round-1 display rule: "fresh" when any
    /// fresh engine row exists (fresh supersedes legacy), "legacy" when
    /// only legacy-import rows exist, None when the game has no evals.
    pub analysis_kind: Option<&'static str>,
    /// Max stored depth of the fresh analysis (None for legacy/none).
    pub analysis_depth: Option<i64>,
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

    // The list-row extras (source tag, ⑂ dup flag, analysis presence) ride
    // as correlated subqueries: they run only for the returned page (≤ 500
    // rows), each over an indexed lookup — cheap even on 121k games.
    let rows_sql = format!(
        "SELECT g.id,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                g.white_elo, g.black_elo,
                COALESCE(e.name, '?'), g.date, g.result, g.eco, g.ply_count,
                s.name, s.kind,
                EXISTS(SELECT 1 FROM duplicates d WHERE d.kept_game_id = g.id),
                EXISTS(SELECT 1 FROM analyses a
                       WHERE a.game_id = g.id AND a.kind = 'fresh'),
                EXISTS(SELECT 1 FROM analyses a
                       WHERE a.game_id = g.id AND a.kind = 'legacy-import'),
                (SELECT MAX(a.depth) FROM analyses a
                 WHERE a.game_id = g.id AND a.kind = 'fresh')
         FROM games g
         JOIN sources s ON s.id = g.source_id
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
                let has_fresh: bool = row.get(13)?;
                let has_legacy: bool = row.get(14)?;
                let fresh_depth: Option<i64> = row.get(15)?;
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
                    source: row.get(10)?,
                    source_kind: row.get(11)?,
                    dup: row.get(12)?,
                    analysis_kind: if has_fresh {
                        Some("fresh")
                    } else if has_legacy {
                        Some("legacy")
                    } else {
                        None
                    },
                    analysis_depth: if has_fresh { fresh_depth } else { None },
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
    /// Resolved opening name for `eco` (bundled CC0 dataset); None when the
    /// game has no ECO or the code is unknown.
    pub opening_name: Option<String>,
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

    let opening_name = match eco.as_deref() {
        Some(code) => kibitz_db::eco::name_for(conn, code).map_err(|e| e.to_string())?,
        None => None,
    };

    let start: Board = match start_fen.as_deref() {
        Some(fen) => fen
            .parse()
            .map_err(|e| format!("game {id} has a bad start FEN {fen:?}: {e:?}"))?,
        None => Board::default(),
    };
    let moves = kibitz_db::movebin::decode_game(&start, &movetext)
        .map_err(|e| format!("game {id} movetext failed to decode: {e}"))?;
    let mut board = start;
    let mut sans = Vec::with_capacity(moves.len());
    for mv in moves {
        sans.push(kibitz_db::san::format_san(&board, mv));
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
        opening_name,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningTree {
    pub rows: Vec<TreeRow>,
    /// Pure query time in milliseconds (measured, a product claim — never
    /// estimated). Excludes FEN parsing.
    pub elapsed_ms: f64,
}

/// Opening tree for `fen`: every continuation played from this position in
/// the database, with counts, W/D/L (White's perspective), avg mover elo,
/// performance rating and the measured query time.
#[tauri::command]
pub async fn opening_tree(state: State<'_, DbState>, fen: String) -> Result<OpeningTree, String> {
    with_conn(&state, |conn| {
        let (moves, elapsed) =
            kibitz_db::query::opening_tree(conn, &fen).map_err(|e| e.to_string())?;
        Ok(OpeningTree {
            rows: moves
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
                .collect(),
            elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        })
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
    /// Pure query time in milliseconds (measured — the position-search
    /// header's "N GAMES · M ms" pill is a product claim, never estimated).
    pub elapsed_ms: f64,
}

/// Games whose mainline reached the position `fen` (position-hash lookup).
#[tauri::command]
pub async fn find_games_at(state: State<'_, DbState>, fen: String) -> Result<GamesAt, String> {
    with_conn(&state, |conn| {
        let (hits, elapsed) = kibitz_db::query::find_fen(conn, &fen).map_err(|e| e.to_string())?;
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
        Ok(GamesAt {
            total,
            rows,
            elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        })
    })
}

/// Resolve ECO codes to canonical opening names for UI-side display (SRS
/// browser, weak-line cards, ...). Unknown codes map to null.
pub(crate) fn eco_names_impl(
    conn: &Connection,
    codes: &[String],
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    // The openings table is populated at import time; make sure it exists
    // for databases that somehow skipped that (cheap when already loaded).
    kibitz_db::eco::ensure_openings(conn).map_err(|e| e.to_string())?;
    let mut out = std::collections::HashMap::with_capacity(codes.len());
    for code in codes {
        let name = kibitz_db::eco::name_for(conn, code).map_err(|e| e.to_string())?;
        out.insert(code.clone(), name);
    }
    Ok(out)
}

/// Map of ECO code → opening name (null when unknown).
#[tauri::command]
pub async fn eco_names(
    state: State<'_, DbState>,
    codes: Vec<String>,
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    with_conn(&state, |conn| eco_names_impl(conn, &codes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
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
        let conn = kibitz_db::db::open(&dir.path().join("test.sqlite")).unwrap();
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
    fn busy_errors_retry_with_the_documented_backoff_then_succeed() {
        let mut calls = 0u32;
        let mut slept: Vec<u64> = Vec::new();
        let out = retry_busy(
            || {
                calls += 1;
                if calls < 3 {
                    Err("database is locked".to_string())
                } else {
                    Ok(42)
                }
            },
            &mut |d| slept.push(d.as_millis() as u64),
        );
        assert_eq!(out.unwrap(), 42);
        assert_eq!(calls, 3);
        assert_eq!(slept, vec![50, 100], "one sleep per failed attempt");
    }

    #[test]
    fn busy_retries_are_bounded_and_real_errors_never_retry() {
        // Permanently busy: bounded attempts, then the error surfaces.
        let mut calls = 0u32;
        let out: Result<(), String> = retry_busy(
            || {
                calls += 1;
                Err("database is locked".to_string())
            },
            &mut |_| {},
        );
        assert!(is_busy_msg(&out.unwrap_err()));
        assert_eq!(calls, 1 + super::BUSY_RETRY_DELAYS_MS.len() as u32);

        // A real error passes through on the first attempt.
        let mut calls = 0u32;
        let out: Result<(), String> = retry_busy(
            || {
                calls += 1;
                Err("no game with id 7".to_string())
            },
            &mut |_| {},
        );
        assert_eq!(out.unwrap_err(), "no game with id 7");
        assert_eq!(calls, 1);
    }

    /// Pins two things against REAL SQLite: (a) rusqlite's message text for
    /// SQLITE_BUSY matches `is_busy_msg`, and (b) a command racing a write
    /// transaction on another connection (the TWIC/jobs worker pattern)
    /// succeeds via retry instead of failing (audit #2/#7).
    #[test]
    fn contended_write_lock_is_retried_until_the_writer_commits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.sqlite");
        let writer = kibitz_db::db::open(&path).unwrap();
        let reader = kibitz_db::db::open(&path).unwrap();
        // No busy_timeout on `reader`: the first attempts fail immediately.

        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        // Sanity: without retry the write fails with the busy message.
        let direct = reader
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('x', '1')",
                [],
            )
            .map_err(|e| e.to_string())
            .unwrap_err();
        assert!(is_busy_msg(&direct), "unexpected message: {direct}");

        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            writer.execute_batch("COMMIT").unwrap();
        });
        let out = retry_busy(
            || {
                reader
                    .execute(
                        "INSERT OR REPLACE INTO meta (key, value) VALUES ('x', '2')",
                        [],
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            &mut std::thread::sleep,
        );
        handle.join().unwrap();
        assert!(out.is_ok(), "retry should outlast the writer: {out:?}");
    }

    #[test]
    fn db_summary_reports_fresh_counts_and_the_open_path() {
        let (_dir, conn) = fixture_db();
        let s = db_summary_impl(&conn).unwrap();
        assert_eq!(s.games, 3);
        assert_eq!(s.sources, 1);
        assert!(s.positions > 0);
        assert!(s.path.ends_with("test.sqlite"), "{}", s.path);
        // The counts are live: another import moves them (the single
        // count source all displays share must never be a stale snapshot).
        conn.execute(
            "INSERT INTO sources (name, origin, license, kind)
             VALUES ('extra', 'unit test', 'pd', 'twic')",
            [],
        )
        .unwrap();
        assert_eq!(db_summary_impl(&conn).unwrap().sources, 2);
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

    #[test]
    fn get_game_resolves_the_opening_name() {
        let (_dir, conn) = fixture_db();
        // The Opera game is ECO-tagged C41 at import; the header shows the
        // resolved base name (run-6 deviation "no opening name" dies here).
        let g = get_game_impl(&conn, 1).unwrap();
        assert_eq!(g.eco.as_deref(), Some("C41"));
        assert_eq!(g.opening_name.as_deref(), Some("Philidor Defense"));
        // A game without a recognizable opening keeps None, not a fake.
        let mini = get_game_impl(&conn, 2).unwrap();
        if mini.eco.is_none() {
            assert_eq!(mini.opening_name, None);
        }
    }

    #[test]
    fn eco_names_resolves_known_codes_and_nulls_unknown() {
        let (_dir, conn) = fixture_db();
        let out = eco_names_impl(
            &conn,
            &["C41".to_string(), "B01".to_string(), "Z99".to_string()],
        )
        .unwrap();
        assert_eq!(out.get("C41").unwrap().as_deref(), Some("Philidor Defense"));
        assert_eq!(
            out.get("B01").unwrap().as_deref(),
            Some("Scandinavian Defense")
        );
        assert_eq!(out.get("Z99").unwrap(), &None);
        assert_eq!(kibitz_db::engine::spawn_count(), 0);
    }

    #[test]
    fn search_and_tree_report_measured_timing() {
        let (_dir, conn) = fixture_db();
        // Impl-level check via kibitz-db (the command wrappers only convert
        // the Duration): both queries return a real measured duration.
        let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let (hits, elapsed) = kibitz_db::query::find_fen(&conn, start).unwrap();
        assert_eq!(hits.len(), 3, "every game reaches the start position");
        assert!(elapsed.as_secs_f64() >= 0.0);
        let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let (hits, elapsed) = kibitz_db::query::find_fen(&conn, after_e4).unwrap();
        assert_eq!(hits.len(), 1, "only the Opera game opens 1.e4");
        assert!(elapsed.as_secs_f64() * 1000.0 < 10_000.0, "sane magnitude");
    }

    #[test]
    fn list_games_carries_source_dup_and_analysis_fields() {
        let (_dir, conn) = fixture_db();
        let all = list_games_impl(&conn, &GameFilter::default(), 0, 50).unwrap();

        // Fresh fixture: real source tag, no dup links, no analyses.
        for row in &all.rows {
            assert_eq!(row.source, "fixture");
            assert_eq!(row.source_kind, "personal");
            assert!(!row.dup);
            assert_eq!(row.analysis_kind, None);
            assert_eq!(row.analysis_depth, None);
        }

        // Game 1 gains a legacy eval, then a fresh one; game 2 a dup link.
        conn.execute(
            "INSERT INTO analyses (game_id, ply, kind, engine, depth, eval_cp)
             VALUES (1, 4, 'legacy-import', 'Rybka 4', 18, 35)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO duplicates (kept_game_id, source_id) VALUES (2, 1)",
            [],
        )
        .unwrap();
        let rows = list_games_impl(&conn, &GameFilter::default(), 0, 50)
            .unwrap()
            .rows;
        let g1 = rows.iter().find(|r| r.id == 1).unwrap();
        assert_eq!(g1.analysis_kind, Some("legacy"));
        assert_eq!(g1.analysis_depth, None, "depth shows for fresh only");
        let g2 = rows.iter().find(|r| r.id == 2).unwrap();
        assert!(g2.dup, "⑂ flag from the duplicates link");

        // Fresh supersedes legacy in the display rule.
        conn.execute(
            "INSERT INTO analyses (game_id, ply, kind, engine, depth, eval_cp)
             VALUES (1, 4, 'fresh', 'Stockfish 18', 24, 30)",
            [],
        )
        .unwrap();
        let rows = list_games_impl(&conn, &GameFilter::default(), 0, 50)
            .unwrap()
            .rows;
        let g1 = rows.iter().find(|r| r.id == 1).unwrap();
        assert_eq!(g1.analysis_kind, Some("fresh"));
        assert_eq!(g1.analysis_depth, Some(24));

        // Wire shape: camelCase keys for the new fields.
        let json = serde_json::to_string(&g1).unwrap();
        for needle in [
            "\"sourceKind\":",
            "\"dup\":",
            "\"analysisKind\":",
            "\"analysisDepth\":",
        ] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }
    }
}
