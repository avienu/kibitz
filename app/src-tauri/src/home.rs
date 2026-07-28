//! Home-screen and schedule IPC (round-2 items 7 + 8): the Direction-A
//! Home surface built ONLY from honest data — a persisted last-game
//! pointer, source-dated new games, findings from a cached profile (never
//! built on the fly), real due counts and real queue state — plus the
//! meta-backed commitment ("Club night · Thursday") and prep-state
//! settings the greeting uses. Home degrades honestly: absent data is
//! null/false/empty, never invented (maintainer ruling).
//!
//! Everything here is read/write on the open database's `meta` table and
//! read-only elsewhere. The engine is never involved (CLAUDE.md #6).

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::browse::{with_conn, DbState};
use crate::dbops::JobsWorker;

// ---------------------------------------------------------------------------
// meta helpers
// ---------------------------------------------------------------------------

fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())
}

/// Set (or, with `None`, delete) one meta key.
fn meta_set(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(v) => conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = ?2",
                rusqlite::params![key, v],
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
        None => conn
            .execute("DELETE FROM meta WHERE key = ?1", [key])
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

fn now_utc(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT datetime('now')", [], |r| r.get(0))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// item 7: commitment setting (meta keys commitment_label / commitment_opponent)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commitment {
    /// Free text, e.g. "Club night · Thursday". Null = not set.
    pub label: Option<String>,
    /// Optional opponent name the commitment is against. Null = not set.
    pub opponent: Option<String>,
}

pub(crate) fn commitment_get_impl(conn: &Connection) -> Result<Commitment, String> {
    Ok(Commitment {
        label: meta_get(conn, "commitment_label")?,
        opponent: meta_get(conn, "commitment_opponent")?,
    })
}

/// The stored commitment; absent keys are null.
#[tauri::command]
pub async fn commitment_get(state: State<'_, DbState>) -> Result<Commitment, String> {
    with_conn(&state, commitment_get_impl)
}

pub(crate) fn commitment_set_impl(
    conn: &Connection,
    label: Option<String>,
    opponent: Option<String>,
) -> Result<Commitment, String> {
    meta_set(conn, "commitment_label", label.as_deref())?;
    meta_set(conn, "commitment_opponent", opponent.as_deref())?;
    commitment_get_impl(conn)
}

/// Persist the commitment; passing null clears a field. Returns the stored
/// state.
#[tauri::command]
pub async fn commitment_set(
    state: State<'_, DbState>,
    label: Option<String>,
    opponent: Option<String>,
) -> Result<Commitment, String> {
    // Clones because with_conn's closure may be re-called on busy retry.
    with_conn(&state, |conn| {
        commitment_set_impl(conn, label.clone(), opponent.clone())
    })
}

// ---------------------------------------------------------------------------
// prep state (meta key prep_state): "no prep started for X yet" is real
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepEntry {
    pub opponent: String,
    /// "white" | "black" — the color the prep targets.
    pub color: String,
    /// UTC "YYYY-MM-DD HH:MM:SS" of when the prep was started.
    pub started_at: String,
}

pub(crate) fn prep_state_get_impl(conn: &Connection) -> Result<Vec<PrepEntry>, String> {
    match meta_get(conn, "prep_state")? {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("stored prep_state: {e}")),
    }
}

/// Preps the user has started ({opponent, color, startedAt}); empty by
/// default.
#[tauri::command]
pub async fn prep_state_get(state: State<'_, DbState>) -> Result<Vec<PrepEntry>, String> {
    with_conn(&state, prep_state_get_impl)
}

pub(crate) fn prep_state_set_impl(conn: &Connection, entries: &[PrepEntry]) -> Result<(), String> {
    let json = serde_json::to_string(entries).map_err(|e| e.to_string())?;
    meta_set(conn, "prep_state", Some(&json))
}

/// Replace the stored prep-state list.
#[tauri::command]
pub async fn prep_state_set(
    state: State<'_, DbState>,
    entries: Vec<PrepEntry>,
) -> Result<(), String> {
    with_conn(&state, |conn| prep_state_set_impl(conn, &entries))
}

// ---------------------------------------------------------------------------
// last game (meta key last_game) — the Continue card
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastGameMeta {
    game_id: i64,
    ply: i64,
    /// Board orientation when the user left (resume restores it).
    #[serde(default)]
    flipped: bool,
    opened_at: String,
}

pub(crate) fn touch_last_game_impl(
    conn: &Connection,
    game_id: i64,
    ply: i64,
    flipped: bool,
) -> Result<(), String> {
    let meta = LastGameMeta {
        game_id,
        ply,
        flipped,
        opened_at: now_utc(conn)?,
    };
    meta_set(
        conn,
        "last_game",
        Some(&serde_json::to_string(&meta).map_err(|e| e.to_string())?),
    )
}

/// Raw last-game pointer for session restore at launch (the Continue
/// card uses the richer `home_summary`; this is the cheap direct read).
/// Verifies the game still exists — a deleted game degrades to None.
#[tauri::command]
pub async fn last_game_get(
    state: State<'_, crate::browse::DbState>,
) -> Result<Option<LastGameMeta>, String> {
    crate::browse::with_conn(&state, |conn| {
        let Some(json) = meta_get(conn, "last_game")? else {
            return Ok(None);
        };
        let Ok(meta) = serde_json::from_str::<LastGameMeta>(&json) else {
            return Ok(None);
        };
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM games WHERE id = ?1", [meta.game_id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(exists.map(|_| meta))
    })
}

/// Record that the user is viewing `game_id` at `ply` (the UI calls this
/// from the game view; Home's Continue card reads it back).
#[tauri::command]
pub async fn touch_last_game(
    state: State<'_, DbState>,
    game_id: i64,
    ply: i64,
    flipped: Option<bool>,
) -> Result<(), String> {
    with_conn(&state, |conn| {
        touch_last_game_impl(conn, game_id, ply, flipped.unwrap_or(false))
    })
}

// ---------------------------------------------------------------------------
// profile cache (meta key profile_cache_self) — findings read this ONLY
// ---------------------------------------------------------------------------

/// Most-recent games a cached profile build may scan (same cap as the
/// interactive profile command).
const PROFILE_MAX_GAMES: u32 = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedProfileInfo {
    pub player: String,
    pub built_at: String,
}

pub(crate) fn cache_profile_impl(
    conn: &Connection,
    player: &str,
) -> Result<CachedProfileInfo, String> {
    let profile = kibitz_db::profile::build_profile(conn, player, PROFILE_MAX_GAMES)
        .map_err(|e| e.to_string())?;
    let built_at = now_utc(conn)?;
    let envelope = serde_json::json!({
        "player": player,
        "builtAt": built_at,
        "profile": profile,
    });
    meta_set(conn, "profile_cache_self", Some(&envelope.to_string()))?;
    Ok(CachedProfileInfo {
        player: player.to_string(),
        built_at,
    })
}

/// Build the player's profile (static analysis + stored evals; no engine)
/// and cache it for Home's findings panel. The Profile screen calls this
/// when it builds the self profile; `home_summary` only ever reads the
/// cache.
#[tauri::command]
pub async fn cache_profile(
    state: State<'_, DbState>,
    player: String,
) -> Result<CachedProfileInfo, String> {
    with_conn(&state, |conn| cache_profile_impl(conn, &player))
}

// ---------------------------------------------------------------------------
// item 8: home_summary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastGame {
    pub id: i64,
    pub white: String,
    pub black: String,
    /// Ply the user last had on the board.
    pub ply: i64,
    pub opened_at: String,
    pub flipped: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGameRow {
    pub id: i64,
    pub white: String,
    pub black: String,
    pub result: String,
    /// Source name (e.g. "TWIC 1594").
    pub source: String,
    /// Source kind for the tag color: personal | twic | online | other.
    pub source_kind: String,
    /// When the game's SOURCE was imported (UTC). Games carry no per-row
    /// import timestamp, so source-level `sources.imported_at` is the
    /// honest granularity: a source's games count as new for 7 days after
    /// that source was imported.
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Plain-language row label, e.g. "TrappedPiece — allowed against you".
    pub label: String,
    /// Display value (count or percentage), pre-formatted.
    pub value: String,
    /// Supporting-game count behind the claim (drives the evidence aside).
    pub evidence_count: u32,
    /// Stable claim id the Profile screen pre-selects:
    /// "motif:<Kind>:missed" | "motif:<Kind>:allowed" | "structure:<flag>".
    pub claim_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningJobs {
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
    pub worker_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSummary {
    /// Null until the user has opened a game (touch_last_game).
    pub last_game: Option<LastGame>,
    /// Games from sources imported in the last 7 days — personal/online
    /// sources first, then bulk, newest first within each (capped; see
    /// `new_games_total`).
    pub new_games: Vec<NewGameRow>,
    pub new_games_total: i64,
    /// Of those, games from personal/online sources only — the honest
    /// scope for "N games this week" (bulk imports are not "your week").
    pub new_games_personal_total: i64,
    /// True only when a cached profile exists; Home degrades honestly.
    pub findings_available: bool,
    /// Top findings (≤ 4) from the CACHED profile only — empty when no
    /// cache exists. Never built on the fly.
    pub findings: Vec<Finding>,
    /// Who the cached profile is about and when it was built (null without
    /// a cache).
    pub profile_player: Option<String>,
    pub profile_built_at: Option<String>,
    /// Due opening-SRS cards, both colors.
    pub due_srs: u32,
    /// Always null: the tactics queue is endless (weakness-weighted
    /// selection), so there is no honest "due today" count to show.
    pub due_tactics: Option<i64>,
    pub running_jobs: RunningJobs,
}

/// Cap on the "New since …" list; the full count rides in
/// `new_games_total`.
const NEW_GAMES_MAX_ROWS: usize = 8;

fn last_game(conn: &Connection) -> Result<Option<LastGame>, String> {
    let Some(json) = meta_get(conn, "last_game")? else {
        return Ok(None);
    };
    // A corrupt or old-format value must never take Home down: the
    // Continue card simply degrades to absent.
    let Ok(meta) = serde_json::from_str::<LastGameMeta>(&json) else {
        return Ok(None);
    };
    // The pointed-at game may have been deleted since: degrade to null.
    let names: Option<(String, String)> = conn
        .query_row(
            "SELECT COALESCE(wp.name, '?'), COALESCE(bp.name, '?')
             FROM games g
             LEFT JOIN players wp ON wp.id = g.white_id
             LEFT JOIN players bp ON bp.id = g.black_id
             WHERE g.id = ?1",
            [meta.game_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(names.map(|(white, black)| LastGame {
        id: meta.game_id,
        white,
        black,
        ply: meta.ply,
        flipped: meta.flipped,
        opened_at: meta.opened_at,
    }))
}

fn result_str(code: i64) -> &'static str {
    match code {
        1 => "1-0",
        2 => "0-1",
        3 => "1/2-1/2",
        _ => "*",
    }
}

/// New-games data for Home: rows (personal/online sources FIRST — a bulk
/// TWIC week must not drown the user's own games, audit #11), the total,
/// and the personal/online-only total that scopes the "this week" claim.
fn new_games(conn: &Connection) -> Result<(Vec<NewGameRow>, i64, i64), String> {
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM games g JOIN sources s ON s.id = g.source_id
             WHERE s.imported_at >= datetime('now', '-7 days')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let personal_total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM games g JOIN sources s ON s.id = g.source_id
             WHERE s.imported_at >= datetime('now', '-7 days')
               AND s.kind IN ('personal', 'online')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare_cached(
            "SELECT g.id, COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                    g.result, s.name, s.kind, s.imported_at
             FROM games g
             JOIN sources s ON s.id = g.source_id
             LEFT JOIN players wp ON wp.id = g.white_id
             LEFT JOIN players bp ON bp.id = g.black_id
             WHERE s.imported_at >= datetime('now', '-7 days')
             ORDER BY (s.kind IN ('personal', 'online')) DESC, g.id DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([NEW_GAMES_MAX_ROWS as i64], |r| {
            Ok(NewGameRow {
                id: r.get(0)?,
                white: r.get(1)?,
                black: r.get(2)?,
                result: result_str(r.get(3)?).to_string(),
                source: r.get(4)?,
                source_kind: r.get(5)?,
                imported_at: r.get(6)?,
            })
        })
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| e.to_string())?;
    Ok((rows, total, personal_total))
}

/// Top-4 findings from the cached profile JSON: the motif rows with the
/// most misses/allowances (already sorted by the profiler), with the worst
/// underperforming structure (≥ 3 games) mixed in when one exists.
fn findings_from_cache(profile: &serde_json::Value) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    if let Some(motifs) = profile["motifs"].as_array() {
        for m in motifs {
            let kind = m["kind"].as_str().unwrap_or("?");
            let missed = m["missed"].as_u64().unwrap_or(0);
            let allowed = m["allowed"].as_u64().unwrap_or(0);
            let ex_missed = m["example_missed"].as_array().map_or(0, |a| a.len());
            let ex_allowed = m["example_allowed"].as_array().map_or(0, |a| a.len());
            if missed > 0 {
                out.push(Finding {
                    label: format!("{kind} — missed opportunities"),
                    value: missed.to_string(),
                    evidence_count: ex_missed as u32,
                    claim_id: format!("motif:{kind}:missed"),
                });
            }
            if allowed > 0 {
                out.push(Finding {
                    label: format!("{kind} — allowed against you"),
                    value: allowed.to_string(),
                    evidence_count: ex_allowed as u32,
                    claim_id: format!("motif:{kind}:allowed"),
                });
            }
            if out.len() >= 3 {
                break;
            }
        }
    }
    out.truncate(3);
    // Worst structure with a real sample, if any (score below 50%).
    if let Some(structs) = profile["structures"].as_array() {
        let worst = structs
            .iter()
            .filter(|s| s["games"].as_u64().unwrap_or(0) >= 3)
            .filter(|s| s["score_pct"].as_f64().unwrap_or(100.0) < 50.0)
            .min_by(|a, b| {
                a["score_pct"]
                    .as_f64()
                    .unwrap_or(100.0)
                    .total_cmp(&b["score_pct"].as_f64().unwrap_or(100.0))
            });
        if let Some(s) = worst {
            let flag = s["flag"].as_str().unwrap_or("?");
            out.push(Finding {
                label: format!("{flag} games"),
                value: format!("{:.1}%", s["score_pct"].as_f64().unwrap_or(0.0)),
                evidence_count: s["examples"].as_array().map_or(0, |a| a.len()) as u32,
                claim_id: format!("structure:{flag}"),
            });
        }
    }
    out.truncate(4);
    out
}

pub(crate) fn home_summary_impl(
    conn: &Connection,
    worker_active: bool,
) -> Result<HomeSummary, String> {
    let (new_games, new_games_total, new_games_personal_total) = new_games(conn)?;

    // Findings come from the cache ONLY (absent → honest degradation).
    let (findings, profile_player, profile_built_at) = match meta_get(conn, "profile_cache_self")? {
        None => (Vec::new(), None, None),
        Some(json) => {
            let v: serde_json::Value =
                serde_json::from_str(&json).map_err(|e| format!("profile cache: {e}"))?;
            (
                findings_from_cache(&v["profile"]),
                v["player"].as_str().map(str::to_string),
                v["builtAt"].as_str().map(str::to_string),
            )
        }
    };
    let findings_available = profile_built_at.is_some();

    let now = now_utc(conn)?;
    let due_srs = kibitz_db::repertoire::counts(conn, kibitz_profile::Color::White, &now)
        .map_err(|e| e.to_string())?
        .due
        + kibitz_db::repertoire::counts(conn, kibitz_profile::Color::Black, &now)
            .map_err(|e| e.to_string())?
            .due;

    let (pending, running, done, failed) =
        kibitz_db::jobs::counts(conn).map_err(|e| e.to_string())?;

    Ok(HomeSummary {
        last_game: last_game(conn)?,
        new_games,
        new_games_total,
        new_games_personal_total,
        findings_available,
        findings,
        profile_player,
        profile_built_at,
        due_srs,
        due_tactics: None,
        running_jobs: RunningJobs {
            pending,
            running,
            done,
            failed,
            worker_active,
        },
    })
}

/// Everything Home Direction A needs, honest-only (see [`HomeSummary`]).
#[tauri::command]
pub async fn home_summary(
    state: State<'_, DbState>,
    worker: State<'_, JobsWorker>,
) -> Result<HomeSummary, String> {
    let active = worker.active.load(std::sync::atomic::Ordering::SeqCst);
    with_conn(&state, |conn| home_summary_impl(conn, active))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    /// Opera game + a decisive miniature: enough for a profile build and a
    /// multi-row "new games" list.
    const FIXTURE: &str = r#"[Event "Casual Game"]
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

1. f3 e5 2. g4 Qh4# 0-1
"#;

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 2, "failures: {:?}", st.failures);
        (dir, conn)
    }

    #[test]
    fn commitment_round_trips_and_defaults_to_null() {
        let (_dir, conn) = fixture_db();
        let c = commitment_get_impl(&conn).unwrap();
        assert_eq!((c.label, c.opponent), (None, None), "absent by default");

        let c = commitment_set_impl(
            &conn,
            Some("Club night · Thursday".into()),
            Some("R. Halvorsen".into()),
        )
        .unwrap();
        assert_eq!(c.label.as_deref(), Some("Club night · Thursday"));
        assert_eq!(c.opponent.as_deref(), Some("R. Halvorsen"));
        let c = commitment_get_impl(&conn).unwrap();
        assert_eq!(c.label.as_deref(), Some("Club night · Thursday"));

        // Null clears a field.
        let c = commitment_set_impl(&conn, Some("Club night · Thursday".into()), None).unwrap();
        assert_eq!(c.opponent, None);
    }

    #[test]
    fn prep_state_round_trips_and_defaults_to_empty() {
        let (_dir, conn) = fixture_db();
        assert!(prep_state_get_impl(&conn).unwrap().is_empty());
        let entries = vec![PrepEntry {
            opponent: "R. Halvorsen".into(),
            color: "black".into(),
            started_at: "2026-07-20 19:00:00".into(),
        }];
        prep_state_set_impl(&conn, &entries).unwrap();
        let back = prep_state_get_impl(&conn).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].opponent, "R. Halvorsen");
        assert_eq!(back[0].color, "black");
        // Wire shape: camelCase.
        let json = serde_json::to_string(&back).unwrap();
        assert!(json.contains("\"startedAt\":"), "{json}");
    }

    #[test]
    fn home_summary_degrades_honestly_then_fills_from_real_data() {
        let (_dir, conn) = fixture_db();

        // Fresh database: no last game, no findings, nothing invented.
        let s = home_summary_impl(&conn, false).unwrap();
        assert!(s.last_game.is_none());
        assert!(!s.findings_available);
        assert!(s.findings.is_empty());
        assert_eq!(s.profile_built_at, None);
        assert_eq!(s.due_srs, 0);
        assert_eq!(s.due_tactics, None, "tactics have no honest due count");
        assert_eq!(s.running_jobs.pending, 0);
        assert!(!s.running_jobs.worker_active);
        // The fixture source was imported just now: both games are "new".
        assert_eq!(s.new_games_total, 2);
        assert_eq!(s.new_games.len(), 2);
        assert_eq!(s.new_games[0].id, 2, "newest first");
        assert_eq!(s.new_games[0].source_kind, "personal");
        assert_eq!(s.new_games[1].result, "1-0");
        assert!(!s.new_games[0].imported_at.is_empty());

        // Continue card: touch, then read back.
        touch_last_game_impl(&conn, 1, 20, true).unwrap();
        let s = home_summary_impl(&conn, false).unwrap();
        let lg = s.last_game.expect("last game recorded");
        assert_eq!((lg.id, lg.ply), (1, 20));
        assert_eq!(lg.white, "Morphy, Paul");
        assert!(!lg.opened_at.is_empty());

        // Findings appear ONLY after an explicit cache build.
        let info = cache_profile_impl(&conn, "Morphy, Paul").unwrap();
        assert_eq!(info.player, "Morphy, Paul");
        let s = home_summary_impl(&conn, true).unwrap();
        assert!(s.findings_available);
        assert_eq!(s.profile_player.as_deref(), Some("Morphy, Paul"));
        assert!(s.profile_built_at.is_some());
        assert!(s.findings.len() <= 4);
        for f in &s.findings {
            assert!(!f.label.is_empty() && !f.value.is_empty());
            assert!(
                f.claim_id.starts_with("motif:") || f.claim_id.starts_with("structure:"),
                "claim id shape: {}",
                f.claim_id
            );
        }
        assert!(s.running_jobs.worker_active);

        // Wire shape: camelCase keys.
        let json = serde_json::to_string(&s).unwrap();
        for needle in [
            "\"lastGame\":",
            "\"newGames\":",
            "\"newGamesTotal\":",
            "\"findingsAvailable\":",
            "\"dueSrs\":",
            "\"dueTactics\":",
            "\"runningJobs\":",
            "\"workerActive\":",
            "\"sourceKind\":",
        ] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }

        // No engine anywhere on the Home path.
        assert_eq!(kibitz_db::engine::spawn_count(), 0);
    }

    #[test]
    fn new_games_put_personal_sources_first_and_scope_the_personal_total() {
        let (_dir, conn) = fixture_db();
        // A bulk TWIC source lands AFTER the personal fixture (higher game
        // ids) — without the kind ordering it would drown the list.
        let twic = SourceInfo {
            name: "TWIC 1600".into(),
            origin: "unit test".into(),
            license: "TWIC personal use — not redistributable".into(),
            kind: SourceKind::Twic,
        };
        let bulk_pgn = r#"[Event "Bulk Open"]
[White "Stranger, A."]
[Black "Stranger, B."]
[Result "1/2-1/2"]

1. c4 c5 2. Nc3 Nc6 1/2-1/2
"#;
        let st = import_pgn(&conn, &twic, Cursor::new(bulk_pgn)).unwrap();
        assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);

        let s = home_summary_impl(&conn, false).unwrap();
        assert_eq!(s.new_games_total, 3);
        assert_eq!(
            s.new_games_personal_total, 2,
            "the 'this week' claim is scoped to personal/online sources"
        );
        // Personal rows first despite the bulk game's newer id.
        let kinds: Vec<&str> = s.new_games.iter().map(|g| g.source_kind.as_str()).collect();
        assert_eq!(kinds, vec!["personal", "personal", "twic"]);
        assert_eq!(s.new_games[0].id, 2, "newest personal first");
        assert_eq!(s.new_games[2].source, "TWIC 1600");

        // Wire shape for the new field.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"newGamesPersonalTotal\":2"), "{json}");
    }

    #[test]
    fn last_game_pointer_degrades_when_the_game_is_gone() {
        let (_dir, conn) = fixture_db();
        touch_last_game_impl(&conn, 999, 5, false).unwrap();
        let s = home_summary_impl(&conn, false).unwrap();
        assert!(s.last_game.is_none(), "dangling pointer degrades to null");
    }
}
