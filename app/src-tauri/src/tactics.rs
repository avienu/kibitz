//! Tactics trainer IPC commands (ROADMAP Phase 5) over kibitz_db::tactics:
//! puzzle import, drill selection (rated / motif / weakness-weighted /
//! Woodpecker / speed), solve verification and attempt recording.
//!
//! Engine-off principle (CLAUDE.md #6): nothing here can spawn an engine —
//! solve checking is static (exact match + cozy-chess mate verification)
//! and selection is pure SQL. The import command opens its own connection
//! on a worker thread so the multi-minute 5M-row import never blocks the
//! UI connection (WAL allows the concurrent reader).

use std::path::PathBuf;

use kibitz_db::tactics::{
    self, AttemptOutcome, CycleStats, MotifWeight, MoveVerdict, PuzzleRow, TacticsRating,
    ThemeCount, WoodpeckerSet,
};
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};

/// Time-derived selection seed (tests in kibitz-db pin their own seeds).
fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5eed)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticsState {
    pub rating: f64,
    pub attempts: u32,
    pub puzzles: i64,
    /// Theme tags present in the imported set, most frequent first.
    pub themes: Vec<ThemeCount>,
}

/// Rating + inventory summary for the Tactics tab.
#[tauri::command]
pub async fn tactics_state(state: State<'_, DbState>) -> Result<TacticsState, String> {
    with_conn(&state, |conn| {
        let TacticsRating { rating, attempts } =
            tactics::tactics_rating(conn).map_err(|e| e.to_string())?;
        Ok(TacticsState {
            rating,
            attempts,
            puzzles: tactics::puzzle_count(conn).map_err(|e| e.to_string())?,
            themes: tactics::theme_list(conn).map_err(|e| e.to_string())?,
        })
    })
}

/// Resolve `raw` against the cwd and its ancestors (same convention as the
/// database opener), so repo-root-relative paths work from `app/src-tauri`.
fn resolve_csv_path(raw: &str) -> Result<PathBuf, String> {
    let p = std::path::Path::new(raw);
    if p.is_absolute() {
        return if p.is_file() {
            Ok(p.to_path_buf())
        } else {
            Err(format!("puzzle CSV not found: {raw}"))
        };
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    for base in cwd.ancestors() {
        let cand = base.join(p);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(format!("puzzle CSV not found: {raw}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPuzzlesSummary {
    pub imported: u64,
    pub duplicates_skipped: u64,
    pub filtered_out: u64,
    pub malformed: u64,
    pub elapsed_ms: u64,
}

/// Import the Lichess puzzle CSV (CC0). Runs on a worker thread with a
/// dedicated connection; returns when the import finishes.
#[tauri::command]
pub async fn tactics_import_puzzles(
    state: State<'_, DbState>,
    path: String,
    min_popularity: Option<i64>,
    max_rows: Option<u64>,
) -> Result<ImportPuzzlesSummary, String> {
    let db_path: String = with_conn(&state, |conn| {
        conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    })?;
    let csv_path = resolve_csv_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn =
            kibitz_db::db::open(std::path::Path::new(&db_path)).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let source = kibitz_db::import::SourceInfo {
            name: "lichess-puzzles".into(),
            origin: "https://database.lichess.org/#puzzles".into(),
            license: "CC0-1.0".into(),
            kind: kibitz_db::import::SourceKind::Other,
        };
        let reader = std::io::BufReader::with_capacity(
            1 << 20,
            std::fs::File::open(&csv_path).map_err(|e| e.to_string())?,
        );
        let opts = tactics::PuzzleImportOptions {
            min_popularity,
            max_rows,
        };
        let st = tactics::import_puzzles_csv(&conn, &source, reader, &opts)
            .map_err(|e| e.to_string())?;
        Ok(ImportPuzzlesSummary {
            imported: st.imported,
            duplicates_skipped: st.duplicates_skipped,
            filtered_out: st.filtered_out,
            malformed: st.malformed,
            elapsed_ms: st.elapsed.as_millis() as u64,
        })
    })
    .await
    .map_err(|e| format!("import worker panicked: {e}"))?
}

/// One served puzzle plus (for weakness mode) the explanation of WHY it
/// was chosen — the UI must be able to show the reason.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedPuzzle {
    pub puzzle: PuzzleRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub matched_themes: Vec<String>,
    pub allowed: u32,
    pub missed: u32,
}

impl From<PuzzleRow> for ServedPuzzle {
    fn from(puzzle: PuzzleRow) -> Self {
        ServedPuzzle {
            puzzle,
            motif: None,
            reason: None,
            matched_themes: Vec::new(),
            allowed: 0,
            missed: 0,
        }
    }
}

/// Serve the next puzzle. `mode`: rated | motif (requires `theme`) |
/// weakness (uses `weights`, the profile's motif rows) | speed. The target
/// rating defaults to the user's current tactics rating.
#[tauri::command]
pub async fn tactics_next_puzzle(
    state: State<'_, DbState>,
    mode: String,
    theme: Option<String>,
    weights: Option<Vec<MotifWeight>>,
) -> Result<Option<ServedPuzzle>, String> {
    let seed = time_seed();
    with_conn(&state, |conn| {
        let target = tactics::tactics_rating(conn)
            .map_err(|e| e.to_string())?
            .rating
            .round() as i64;
        match mode.as_str() {
            "rated" => Ok(tactics::next_rated(conn, target, seed)
                .map_err(|e| e.to_string())?
                .map(Into::into)),
            "motif" => {
                // as_deref, not move: with_conn may re-call on busy retry.
                let theme = theme.as_deref().ok_or("motif mode requires a theme")?;
                Ok(tactics::next_by_theme(conn, target, theme, seed)
                    .map_err(|e| e.to_string())?
                    .map(Into::into))
            }
            "speed" => Ok(tactics::next_speed(conn, target, seed)
                .map_err(|e| e.to_string())?
                .map(Into::into)),
            "weakness" => {
                let weights = weights.clone().unwrap_or_default();
                Ok(
                    tactics::next_weakness_weighted(conn, target, &weights, seed)
                        .map_err(|e| e.to_string())?
                        .map(|c| ServedPuzzle {
                            puzzle: c.puzzle,
                            motif: c.motif,
                            reason: Some(c.reason),
                            matched_themes: c.matched_themes,
                            allowed: c.allowed,
                            missed: c.missed,
                        }),
                )
            }
            other => Err(format!("unknown drill mode {other:?}")),
        }
    })
}

/// Check one solver move: "correct" | "correctAltMate" | "wrong".
/// Static verification only (cozy-chess mate check) — no engine.
#[tauri::command]
pub fn tactics_verify_move(
    fen: String,
    expected: String,
    played: String,
) -> Result<String, String> {
    let verdict = tactics::verify_move(&fen, &expected, &played).map_err(|e| e.to_string())?;
    Ok(match verdict {
        MoveVerdict::Correct => "correct",
        MoveVerdict::CorrectAltMate => "correctAltMate",
        MoveVerdict::Wrong => "wrong",
    }
    .to_string())
}

/// Record an attempt; returns the rating movement (zero in non-rated modes).
#[tauri::command]
pub async fn tactics_record_attempt(
    state: State<'_, DbState>,
    puzzle_id: i64,
    solved: bool,
    time_ms: i64,
    mode: String,
    cycle_id: Option<i64>,
) -> Result<AttemptOutcome, String> {
    with_conn(&state, |conn| {
        tactics::record_attempt(conn, puzzle_id, solved, time_ms, &mode, cycle_id)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn tactics_woodpecker_sets(
    state: State<'_, DbState>,
) -> Result<Vec<WoodpeckerSet>, String> {
    with_conn(&state, |conn| {
        tactics::woodpecker_sets(conn).map_err(|e| e.to_string())
    })
}

/// Create a fixed Woodpecker set of `size` puzzles near the user's rating.
#[tauri::command]
pub async fn tactics_create_woodpecker_set(
    state: State<'_, DbState>,
    name: String,
    size: u32,
) -> Result<i64, String> {
    let seed = time_seed();
    with_conn(&state, |conn| {
        let target = tactics::tactics_rating(conn)
            .map_err(|e| e.to_string())?
            .rating
            .round() as i64;
        tactics::create_woodpecker_set(conn, &name, size, target, seed).map_err(|e| e.to_string())
    })
}

/// The set's puzzles in solve order.
#[tauri::command]
pub async fn tactics_woodpecker_puzzles(
    state: State<'_, DbState>,
    set_id: i64,
) -> Result<Vec<PuzzleRow>, String> {
    with_conn(&state, |conn| {
        tactics::woodpecker_set_puzzles(conn, set_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|id| tactics::load_puzzle(conn, id).map_err(|e| e.to_string()))
            .collect()
    })
}

#[tauri::command]
pub async fn tactics_start_cycle(state: State<'_, DbState>, set_id: i64) -> Result<i64, String> {
    with_conn(&state, |conn| {
        tactics::start_woodpecker_cycle(conn, set_id).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn tactics_finish_cycle(state: State<'_, DbState>, cycle_id: i64) -> Result<(), String> {
    with_conn(&state, |conn| {
        tactics::finish_woodpecker_cycle(conn, cycle_id).map_err(|e| e.to_string())
    })
}

/// Per-cycle stats for cycle-over-cycle comparison.
#[tauri::command]
pub async fn tactics_cycle_stats(
    state: State<'_, DbState>,
    set_id: i64,
) -> Result<Vec<CycleStats>, String> {
    with_conn(&state, |conn| {
        tactics::woodpecker_cycle_stats(conn, set_id).map_err(|e| e.to_string())
    })
}
