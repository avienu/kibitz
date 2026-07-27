//! Run-9 UI wiring for the network ingestion surfaces: the TWIC catalog
//! screen and the Account syncs screen (maintainer ruling: both must be
//! real UI, not CLI pointers).
//!
//! All downloads run on ONE background worker thread ([`NetWorker`],
//! `JobsWorker`-like, never the engine worker), strictly serially, with
//! progress polled via `net_progress` and a cooperative cancel between
//! issues (`net_cancel`, like `batch_pause`). The kibitz-db clients keep
//! every posture they already had: TWIC resumes from `twic_issues`, never
//! fetches an issue twice, respects 429s, and its data is personal-use
//! only; the account clients resume from their per-username meta keys.
//!
//! Network requests happen ONLY on explicit user action (Refresh catalog,
//! Download, Sync now) or the user-enabled TWIC auto-sync toggle checked
//! at database-open time.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use kibitz_db::net::{self, Fetcher};
use kibitz_db::twic;

use crate::browse::{with_conn, DbState};

/// Meta key: "1" when TWIC auto-download of new issues is enabled.
const META_AUTO_SYNC: &str = "twic_auto_sync";
/// Meta key: "1" once the user acknowledged the TWIC first-run notice.
const META_NOTICE_ACK: &str = "twic_notice_ack";
/// Meta key: newest TWIC issue confirmed published by a catalog probe.
const META_LATEST_KNOWN: &str = "twic_latest_known";

const SERVICES: [&str; 3] = ["lichess", "chesscom", "fics"];

/// Meta key holding the persisted username for a service's sync card.
/// Distinct from the clients' own resume keys (`lichess_since_{user}`,
/// `chesscom_last_month_{user}`), which stay keyed by username so the
/// resume state is reused automatically whenever the same name is synced.
fn user_key(service: &str) -> String {
    format!("sync_user_{service}")
}

/// Meta key holding the last sync report (JSON) for a service.
fn report_key(service: &str) -> String {
    format!("sync_last_{service}")
}

/// Network-worker state: one background thread for ALL network ingestion
/// (TWIC downloads, account syncs), so requests stay strictly serial
/// app-wide. `stop` is the cooperative cancel flag — the TWIC worker
/// checks it between issues; a single-request account sync cannot be
/// interrupted honestly and ignores it.
#[derive(Default)]
pub struct NetWorker {
    pub active: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
    pub progress: Arc<Mutex<Option<NetProgress>>>,
}

/// Progress snapshot polled by the frontend (`net_progress`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetProgress {
    /// "twic" | "twic-auto" | "lichess" | "chesscom" | "fics".
    pub kind: String,
    /// Human label, e.g. "TWIC download" or "Lichess: username".
    pub label: String,
    /// Items finished (TWIC: issues; account syncs: always 0).
    pub done: u32,
    /// Item count; 0 = indeterminate (single-request account syncs).
    pub total: u32,
    pub detail: String,
    /// True while the worker thread is still running this job.
    pub active: bool,
    pub error: Option<String>,
}

fn set_progress(slot: &Mutex<Option<NetProgress>>, p: NetProgress) {
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(p);
    }
}

fn update_progress(slot: &Mutex<Option<NetProgress>>, f: impl FnOnce(&mut NetProgress)) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(p) = guard.as_mut() {
            f(p);
        }
    }
}

/// Path of the open database (so workers can open their own connection,
/// leaving the UI connection free for polling — same trick as `run_jobs`).
fn open_db_path(state: &State<'_, DbState>) -> Result<String, String> {
    with_conn(state, |conn| {
        conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    })
}

fn worker_conn(db_path: &str) -> Result<Connection, String> {
    let conn = kibitz_db::db::open(Path::new(db_path)).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// `datetime('now')` from SQLite — the same clock every other timestamp
/// in the database uses.
fn now_utc(conn: &Connection) -> String {
    conn.query_row("SELECT datetime('now')", [], |r| r.get(0))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// TWIC catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwicCatalogRow {
    pub issue: u32,
    pub imported: bool,
    /// Games imported from this issue (null when not imported).
    pub games: Option<i64>,
    /// Approximate publication Monday ("YYYY-MM-DD") — weekly arithmetic
    /// from the documented anchor in kibitz-db::twic; label it "approx".
    pub approx_date: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwicCatalog {
    /// Earliest issue the zip archive serves (twic::FIRST_AVAILABLE_ISSUE).
    pub first_available: u32,
    pub latest_imported: Option<u32>,
    /// max(latest imported, newest probe-confirmed issue); null until
    /// something is imported or a catalog refresh has run.
    pub latest_known: Option<u32>,
    /// One row per issue, first_available..=latest_known, newest first.
    /// Empty until latest_known is known.
    pub rows: Vec<TwicCatalogRow>,
    pub auto_sync: bool,
    pub notice_acknowledged: bool,
    /// The exact kibitz-db FIRST_RUN_NOTICE text, for the acknowledge
    /// dialog (single source of truth — never restated in the frontend).
    pub first_run_notice: String,
}

fn meta_u32(conn: &Connection, key: &str) -> Option<u32> {
    net::meta_get(conn, key)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
}

fn meta_flag(conn: &Connection, key: &str) -> bool {
    net::meta_get(conn, key).ok().flatten().as_deref() == Some("1")
}

pub(crate) fn twic_catalog_impl(conn: &Connection) -> Result<TwicCatalog, String> {
    let imported: Vec<(u32, i64)> = twic::imported_issues(conn).map_err(|e| e.to_string())?;
    let latest_imported = imported.last().map(|(issue, _)| *issue);
    let probed = meta_u32(conn, META_LATEST_KNOWN);
    let latest_known = match (latest_imported, probed) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let games_by_issue: std::collections::HashMap<u32, i64> = imported.into_iter().collect();
    let rows = latest_known.map_or_else(Vec::new, |latest| {
        (twic::FIRST_AVAILABLE_ISSUE..=latest)
            .rev()
            .map(|issue| {
                let games = games_by_issue.get(&issue).copied();
                TwicCatalogRow {
                    issue,
                    imported: games.is_some(),
                    games,
                    approx_date: twic::approx_date(issue),
                }
            })
            .collect()
    });
    Ok(TwicCatalog {
        first_available: twic::FIRST_AVAILABLE_ISSUE,
        latest_imported,
        latest_known,
        rows,
        auto_sync: meta_flag(conn, META_AUTO_SYNC),
        notice_acknowledged: meta_flag(conn, META_NOTICE_ACK),
        first_run_notice: twic::FIRST_RUN_NOTICE.to_string(),
    })
}

/// The full TWIC issue catalog: every issue from the earliest the archive
/// serves through the latest known, with import status. Reads only the
/// local database — never the network.
#[tauri::command]
pub async fn twic_catalog(state: State<'_, DbState>) -> Result<TwicCatalog, String> {
    with_conn(&state, twic_catalog_impl)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwicRefresh {
    pub latest_known: Option<u32>,
    /// HEAD requests actually issued (shown to the user for honesty).
    pub requests: u32,
}

/// Discover the newest published issue — EXPLICIT user action only.
/// Issues a handful of HEAD requests (typically 2, hard cap
/// `twic::PROBE_MAX_REQUESTS` = 12) starting from the weekly-arithmetic
/// estimate, honoring 429 backoff, and stores the result in meta.
#[tauri::command]
pub async fn twic_refresh_catalog(state: State<'_, DbState>) -> Result<TwicRefresh, String> {
    let floor = with_conn(&state, |conn| {
        let imported = twic::latest_imported(conn).map_err(|e| e.to_string())?;
        let probed = meta_u32(conn, META_LATEST_KNOWN);
        Ok(imported.max(probed))
    })?;
    let today_days = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        / 86_400) as i64;
    let guess = twic::estimated_issue(today_days);

    // The probe runs without holding the db lock (429 waits can be long).
    let result = twic::probe_latest(&net::UreqFetcher, floor, guess, &mut |d| {
        std::thread::sleep(d)
    })
    .map_err(|e| format!("{e:#}"))?;

    if let Some(latest) = result.latest {
        with_conn(&state, |conn| {
            net::meta_set(conn, META_LATEST_KNOWN, &latest.to_string()).map_err(|e| e.to_string())
        })?;
    }
    Ok(TwicRefresh {
        latest_known: result.latest,
        requests: result.requests,
    })
}

/// Enable/disable auto-download of new TWIC issues at database open.
#[tauri::command]
pub async fn twic_set_auto_sync(state: State<'_, DbState>, enabled: bool) -> Result<(), String> {
    with_conn(&state, |conn| {
        net::meta_set(conn, META_AUTO_SYNC, if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())
    })
}

/// Record that the user acknowledged the TWIC first-run notice.
#[tauri::command]
pub async fn twic_ack_notice(state: State<'_, DbState>) -> Result<(), String> {
    with_conn(&state, |conn| {
        net::meta_set(conn, META_NOTICE_ACK, "1").map_err(|e| e.to_string())
    })
}

/// One TWIC-download worker pass: import `issues` strictly serially on a
/// dedicated connection, updating `progress` per issue and honoring the
/// cooperative `stop` flag between issues. `stop_at_404` selects the
/// auto-sync behavior (a 404 means "caught up", stop) versus the explicit
/// selection behavior (a 404 issue is reported and the rest continue).
fn twic_worker_impl(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    issues: &[u32],
    stop_at_404: bool,
    progress: &Mutex<Option<NetProgress>>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let total = issues.len() as u32;
    let mut imported: u32 = 0;
    let mut games: u64 = 0;
    let mut unavailable: u32 = 0;
    let mut cancelled = false;

    for (i, &issue) in issues.iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        update_progress(progress, |p| {
            p.done = i as u32;
            p.detail = format!("downloading TWIC {issue}…");
        });
        match twic::import_issue(conn, fetcher, issue)
            .map_err(|e| format!("TWIC {issue}: {e:#}"))?
        {
            Some(r) => {
                imported += 1;
                games += r.games_imported;
                update_progress(progress, |p| {
                    p.done = i as u32 + 1;
                    p.detail = format!(
                        "TWIC {issue}: {} games ({} duplicates skipped)",
                        r.games_imported, r.duplicates_skipped
                    );
                });
            }
            None => {
                unavailable += 1;
                update_progress(progress, |p| {
                    p.done = i as u32 + 1;
                    p.detail = format!("TWIC {issue}: not available (404)");
                });
                if stop_at_404 {
                    break;
                }
            }
        }
    }

    let mut summary = format!(
        "{imported} issue{} imported · {games} game{}",
        if imported == 1 { "" } else { "s" },
        if games == 1 { "" } else { "s" }
    );
    if unavailable > 0 {
        summary.push_str(&format!(" · {unavailable} not available"));
    }
    if cancelled {
        let done = imported + unavailable;
        summary.push_str(&format!(" · cancelled ({done} of {total} attempted)"));
    }
    update_progress(progress, |p| {
        p.done = imported + unavailable;
        p.detail = summary;
    });
    Ok(())
}

fn spawn_net_worker(
    worker: &State<'_, NetWorker>,
    initial: NetProgress,
    job: impl FnOnce(&AtomicBool, &Mutex<Option<NetProgress>>) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    if worker.active.swap(true, Ordering::SeqCst) {
        return Err(
            "a download or sync is already running (network jobs are strictly serial)".to_string(),
        );
    }
    worker.stop.store(false, Ordering::SeqCst);
    set_progress(&worker.progress, initial);
    let active = Arc::clone(&worker.active);
    let stop = Arc::clone(&worker.stop);
    let progress = Arc::clone(&worker.progress);
    std::thread::spawn(move || {
        let result = job(&stop, &progress);
        if let Err(e) = &result {
            update_progress(&progress, |p| p.error = Some(e.clone()));
        }
        update_progress(&progress, |p| p.active = false);
        active.store(false, Ordering::SeqCst);
    });
    Ok(())
}

/// Download the given TWIC issues (an explicit selection or "all missing",
/// computed by the frontend from the catalog) on the background worker.
/// Already-imported issues are filtered out (never fetched twice). On the
/// very first download into an empty `twic_issues` table the first-run
/// notice must have been acknowledged. Returns how many issues were queued.
#[tauri::command]
pub async fn twic_download(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
    issues: Vec<u32>,
) -> Result<u32, String> {
    let (db_path, todo) = with_conn(&state, |conn| {
        let nothing_imported = twic::latest_imported(conn)
            .map_err(|e| e.to_string())?
            .is_none();
        if nothing_imported && !meta_flag(conn, META_NOTICE_ACK) {
            return Err(
                "first TWIC download: acknowledge the personal-use notice first".to_string(),
            );
        }
        let imported: std::collections::HashSet<u32> = twic::imported_issues(conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(issue, _)| issue)
            .collect();
        let mut todo: Vec<u32> = issues
            .iter()
            .copied()
            .filter(|i| !imported.contains(i))
            .collect();
        todo.sort_unstable();
        todo.dedup();
        let path: String = conn
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok((path, todo))
    })?;
    if todo.is_empty() {
        return Err("nothing to download: the selected issues are already imported".to_string());
    }

    let count = todo.len() as u32;
    let initial = NetProgress {
        kind: "twic".to_string(),
        label: "TWIC download".to_string(),
        done: 0,
        total: count,
        detail: format!("{count} issue{} queued", if count == 1 { "" } else { "s" }),
        active: true,
        error: None,
    };
    spawn_net_worker(&worker, initial, move |stop, progress| {
        let conn = worker_conn(&db_path)?;
        twic_worker_impl(&conn, &net::UreqFetcher, &todo, false, progress, stop)
    })?;
    Ok(count)
}

/// Issues an auto-sync run would attempt: the next `max_issues` after the
/// newest import — only when auto-sync is on and something is imported
/// (there must be a real resume point; we never guess a starting issue).
pub(crate) fn auto_sync_issues(conn: &Connection) -> Result<Option<Vec<u32>>, String> {
    if !meta_flag(conn, META_AUTO_SYNC) {
        return Ok(None);
    }
    let Some(latest) = twic::latest_imported(conn).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let cap = twic::TwicOptions::default().max_issues;
    Ok(Some((latest + 1..=latest + cap).collect()))
}

/// Database-open hook: when the auto-download toggle is on, quietly fetch
/// NEW issues only (resuming after the newest import, per-run cap = the
/// kibitz-db default of 5, strictly serial, stopping at the first 404).
/// Returns true when a sync was started.
#[tauri::command]
pub async fn twic_auto_sync_check(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
) -> Result<bool, String> {
    let issues = with_conn(&state, auto_sync_issues)?;
    let Some(issues) = issues else {
        return Ok(false);
    };
    if worker.active.load(Ordering::SeqCst) {
        return Ok(false); // never queue behind another network job
    }
    let db_path = open_db_path(&state)?;
    let total = issues.len() as u32;
    let initial = NetProgress {
        kind: "twic-auto".to_string(),
        label: "TWIC auto-sync".to_string(),
        done: 0,
        total,
        detail: "checking for new issues…".to_string(),
        active: true,
        error: None,
    };
    spawn_net_worker(&worker, initial, move |stop, progress| {
        let conn = worker_conn(&db_path)?;
        twic_worker_impl(&conn, &net::UreqFetcher, &issues, true, progress, stop)
    })?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Account syncs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccount {
    pub username: Option<String>,
    /// Last sync report as stored in meta (JSON: at, gamesImported,
    /// duplicatesSkipped, gamesFailed, and per-service extras — or
    /// {at, error} for a failed run). Null before the first sync.
    pub last_report: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAccounts {
    pub lichess: ServiceAccount,
    pub chesscom: ServiceAccount,
    pub fics: ServiceAccount,
}

fn service_account(conn: &Connection, service: &str) -> ServiceAccount {
    let username = net::meta_get(conn, &user_key(service))
        .ok()
        .flatten()
        .filter(|u| !u.is_empty());
    let last_report = net::meta_get(conn, &report_key(service))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok());
    ServiceAccount {
        username,
        last_report,
    }
}

pub(crate) fn sync_accounts_impl(conn: &Connection) -> Result<SyncAccounts, String> {
    Ok(SyncAccounts {
        lichess: service_account(conn, "lichess"),
        chesscom: service_account(conn, "chesscom"),
        fics: service_account(conn, "fics"),
    })
}

/// Per-service card state: persisted username + last sync report.
#[tauri::command]
pub async fn sync_accounts(state: State<'_, DbState>) -> Result<SyncAccounts, String> {
    with_conn(&state, sync_accounts_impl)
}

fn check_service(service: &str) -> Result<(), String> {
    if SERVICES.contains(&service) {
        Ok(())
    } else {
        Err(format!("unknown sync service {service:?}"))
    }
}

pub(crate) fn sync_set_username_impl(
    conn: &Connection,
    service: &str,
    username: &str,
) -> Result<(), String> {
    check_service(service)?;
    let key = user_key(service);
    let trimmed = username.trim();
    if trimmed.is_empty() {
        conn.execute("DELETE FROM meta WHERE key = ?1", [&key])
            .map_err(|e| e.to_string())?;
    } else {
        net::meta_set(conn, &key, trimmed).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Persist (or clear, with an empty string) a service's username.
#[tauri::command]
pub async fn sync_set_username(
    state: State<'_, DbState>,
    service: String,
    username: String,
) -> Result<(), String> {
    with_conn(&state, |conn| {
        sync_set_username_impl(conn, &service, &username)
    })
}

/// Run one account sync on the background worker using the existing
/// kibitz-db client (strictly serial; the clients resume incrementally
/// from their own per-username meta cursors). FICS requires `year`
/// (optionally `month`) — there is no incremental cursor for it. The
/// result (or the error) is persisted as the service's last report.
#[tauri::command]
pub async fn sync_run(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
    service: String,
    username: String,
    year: Option<u16>,
    month: Option<u8>,
) -> Result<(), String> {
    check_service(&service)?;
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("enter a username first".to_string());
    }
    if service == "fics" && year.is_none() {
        return Err(
            "FICS sync needs a year (ficsgames.org serves one year or month per request)"
                .to_string(),
        );
    }
    with_conn(&state, |conn| {
        sync_set_username_impl(conn, &service, &username)
    })?;
    let db_path = open_db_path(&state)?;

    let service_label = match service.as_str() {
        "lichess" => "Lichess",
        "chesscom" => "chess.com",
        _ => "FICS",
    };
    let initial = NetProgress {
        kind: service.clone(),
        label: format!("{service_label}: {username}"),
        done: 0,
        total: 0, // one streaming request+import; no honest fraction exists
        detail: "downloading & importing — strictly serial; rate limits are respected \
                 (a 429 can pause the sync for a minute or more)"
            .to_string(),
        active: true,
        error: None,
    };
    spawn_net_worker(&worker, initial, move |_stop, progress| {
        let conn = worker_conn(&db_path)?;
        let fetcher = net::UreqFetcher;
        let outcome = run_service_sync(&conn, &fetcher, &service, &username, year, month);
        let at = now_utc(&conn);
        let report = match &outcome {
            Ok(report) => {
                let mut r = report.clone();
                r["at"] = serde_json::Value::String(at);
                r
            }
            Err(e) => serde_json::json!({ "at": at, "error": e }),
        };
        let _ = net::meta_set(&conn, &report_key(&service), &report.to_string());
        match outcome {
            Ok(report) => {
                update_progress(progress, |p| p.detail = summarize_report(&report));
                Ok(())
            }
            Err(e) => Err(e),
        }
    })
}

/// Run the actual kibitz-db client and reduce its report to the JSON
/// stored as the service's last report.
fn run_service_sync(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    service: &str,
    username: &str,
    year: Option<u16>,
    month: Option<u8>,
) -> Result<serde_json::Value, String> {
    match service {
        "lichess" => {
            let r = kibitz_db::net::lichess::sync_user(conn, fetcher, username)
                .map_err(|e| format!("{e:#}"))?;
            Ok(serde_json::json!({
                "gamesImported": r.games_imported,
                "duplicatesSkipped": r.duplicates_skipped,
                "gamesFailed": r.games_failed,
            }))
        }
        "chesscom" => {
            let r = kibitz_db::net::chesscom::sync_user(conn, fetcher, username)
                .map_err(|e| format!("{e:#}"))?;
            let (mut imported, mut dups, mut failed) = (0u64, 0u64, 0u64);
            for m in &r.months {
                imported += m.games_imported;
                dups += m.duplicates_skipped;
                failed += m.games_failed;
            }
            Ok(serde_json::json!({
                "gamesImported": imported,
                "duplicatesSkipped": dups,
                "gamesFailed": failed,
                "monthsFetched": r.months.len(),
            }))
        }
        _ => {
            let year = year.expect("checked in sync_run");
            let r = kibitz_db::net::fics::sync_user(conn, fetcher, username, year, month)
                .map_err(|e| format!("{e:#}"))?;
            Ok(serde_json::json!({
                "gamesImported": r.games_imported,
                "duplicatesSkipped": r.duplicates_skipped,
                "gamesFailed": r.games_failed,
                "year": r.year,
                "month": r.month,
                "savedArchive": r.saved_archive.map(|p| p.display().to_string()),
            }))
        }
    }
}

/// One-line human summary of a stored report (worker's final detail).
fn summarize_report(report: &serde_json::Value) -> String {
    let n = |key: &str| report[key].as_u64().unwrap_or(0);
    let mut s = format!(
        "{} imported · {} duplicates · {} failed",
        n("gamesImported"),
        n("duplicatesSkipped"),
        n("gamesFailed")
    );
    if let Some(months) = report["monthsFetched"].as_u64() {
        s.push_str(&format!(" · {months} month(s) fetched"));
    }
    if report["savedArchive"].as_str().is_some() {
        s.push_str(" · bzip2 archive saved (decompress with bunzip2, then Import PGN)");
    }
    s
}

// ---------------------------------------------------------------------------
// Progress polling / cancel / rail badges
// ---------------------------------------------------------------------------

/// Current (or last finished) network job progress; null before any job.
#[tauri::command]
pub async fn net_progress(worker: State<'_, NetWorker>) -> Result<Option<NetProgress>, String> {
    Ok(worker
        .progress
        .lock()
        .map_err(|_| "net progress poisoned".to_string())?
        .clone())
}

/// Cooperative cancel: the TWIC worker stops after the current issue
/// (everything imported so far stays imported — the catalog is the
/// resumable state). Returns false when nothing was running.
#[tauri::command]
pub async fn net_cancel(worker: State<'_, NetWorker>) -> Result<bool, String> {
    let was_active = worker.active.load(Ordering::SeqCst);
    if was_active {
        worker.stop.store(true, Ordering::SeqCst);
    }
    Ok(was_active)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetBadges {
    /// Newest imported TWIC issue (the rail's "wk NNNN"); null when none.
    pub twic_latest_imported: Option<u32>,
    /// Number of services with a configured username (0–3).
    pub accounts_configured: u32,
}

pub(crate) fn rail_net_badges_impl(conn: &Connection) -> Result<NetBadges, String> {
    let twic_latest_imported = twic::latest_imported(conn).map_err(|e| e.to_string())?;
    let accounts_configured = SERVICES
        .iter()
        .filter(|s| {
            net::meta_get(conn, &user_key(s))
                .ok()
                .flatten()
                .is_some_and(|u| !u.is_empty())
        })
        .count() as u32;
    Ok(NetBadges {
        twic_latest_imported,
        accounts_configured,
    })
}

/// Cheap data for the rail badges (TWIC week + configured account count).
#[tauri::command]
pub async fn rail_net_badges(state: State<'_, DbState>) -> Result<NetBadges, String> {
    with_conn(&state, rail_net_badges_impl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::io::Cursor;

    /// Two-game TWIC fixture zip shared with the kibitz-db offline tests.
    const ZIP_A: &[u8] = include_bytes!("../../kibitz-db/tests/fixtures/twic_a.zip");

    fn temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    /// Minimal offline fetcher (same contract as the kibitz-db test
    /// fixture): scripted bodies per URL; unscripted URLs are 404s here.
    #[derive(Default)]
    struct MapFetcher {
        bodies: HashMap<String, Vec<u8>>,
        log: RefCell<Vec<String>>,
    }

    impl Fetcher for MapFetcher {
        fn get(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
        ) -> anyhow::Result<kibitz_db::net::FetchOutcome> {
            self.log.borrow_mut().push(url.to_string());
            Ok(match self.bodies.get(url) {
                Some(b) => kibitz_db::net::FetchOutcome::Body(Box::new(Cursor::new(b.clone()))),
                None => kibitz_db::net::FetchOutcome::NotFound,
            })
        }
        fn post_form(
            &self,
            _url: &str,
            _form: &[(&str, &str)],
        ) -> anyhow::Result<kibitz_db::net::FetchOutcome> {
            anyhow::bail!("no POSTs in these tests")
        }
    }

    fn plant_issue(conn: &Connection, issue: u32, games: i64) {
        conn.execute(
            "INSERT INTO twic_issues (issue, source_id, games) VALUES (?1, NULL, ?2)",
            params![i64::from(issue), games],
        )
        .unwrap();
    }

    #[test]
    fn catalog_empty_db_has_no_rows_but_carries_the_notice() {
        let (_dir, conn) = temp_db();
        let c = twic_catalog_impl(&conn).unwrap();
        assert_eq!(c.first_available, twic::FIRST_AVAILABLE_ISSUE);
        assert_eq!(c.latest_imported, None);
        assert_eq!(c.latest_known, None);
        assert!(c.rows.is_empty(), "no fake rows before anything is known");
        assert!(!c.auto_sync);
        assert!(!c.notice_acknowledged);
        assert_eq!(c.first_run_notice, twic::FIRST_RUN_NOTICE);
    }

    #[test]
    fn catalog_spans_first_available_to_latest_known_newest_first() {
        let (_dir, conn) = temp_db();
        plant_issue(&conn, 1500, 42);
        plant_issue(&conn, 1502, 7);
        net::meta_set(&conn, META_LATEST_KNOWN, "1504").unwrap();
        net::meta_set(&conn, META_AUTO_SYNC, "1").unwrap();
        net::meta_set(&conn, META_NOTICE_ACK, "1").unwrap();

        let c = twic_catalog_impl(&conn).unwrap();
        assert_eq!(c.latest_imported, Some(1502));
        assert_eq!(c.latest_known, Some(1504), "probe result beats imports");
        assert!(c.auto_sync);
        assert!(c.notice_acknowledged);

        let expected_rows = (1504 - twic::FIRST_AVAILABLE_ISSUE + 1) as usize;
        assert_eq!(c.rows.len(), expected_rows);
        assert_eq!(c.rows[0].issue, 1504, "newest first");
        assert_eq!(c.rows.last().unwrap().issue, twic::FIRST_AVAILABLE_ISSUE);

        let row_1500 = c.rows.iter().find(|r| r.issue == 1500).unwrap();
        assert!(row_1500.imported);
        assert_eq!(row_1500.games, Some(42));
        assert_eq!(row_1500.approx_date, twic::approx_date(1500));
        let row_1501 = c.rows.iter().find(|r| r.issue == 1501).unwrap();
        assert!(!row_1501.imported);
        assert_eq!(row_1501.games, None);
    }

    #[test]
    fn catalog_uses_imports_when_no_probe_ran() {
        let (_dir, conn) = temp_db();
        plant_issue(&conn, 1500, 1);
        let c = twic_catalog_impl(&conn).unwrap();
        assert_eq!(c.latest_known, Some(1500));
        assert_eq!(
            c.rows.len(),
            (1500 - twic::FIRST_AVAILABLE_ISSUE + 1) as usize
        );
    }

    #[test]
    fn twic_worker_imports_serially_and_reports_missing_issues() {
        let (_dir, conn) = temp_db();
        let mut fetcher = MapFetcher::default();
        fetcher.bodies.insert(twic::zip_url(1500), ZIP_A.to_vec());
        fetcher.bodies.insert(twic::zip_url(1502), ZIP_A.to_vec());
        // 1501 is unscripted -> 404 (skipped, not fatal, in explicit mode).
        let progress = Mutex::new(Some(NetProgress {
            kind: "twic".into(),
            label: "TWIC download".into(),
            done: 0,
            total: 3,
            detail: String::new(),
            active: true,
            error: None,
        }));
        let stop = AtomicBool::new(false);
        twic_worker_impl(
            &conn,
            &fetcher,
            &[1500, 1501, 1502],
            false,
            &progress,
            &stop,
        )
        .unwrap();

        assert_eq!(
            fetcher.log.borrow().as_slice(),
            &[
                twic::zip_url(1500),
                twic::zip_url(1501),
                twic::zip_url(1502)
            ],
            "strictly serial, ascending"
        );
        // 1502's games are duplicates of 1500's -> 2 issues imported, 2 games.
        assert_eq!(
            twic::imported_issues(&conn)
                .unwrap()
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            vec![1500, 1502]
        );
        let p = progress.lock().unwrap().clone().unwrap();
        assert_eq!(p.done, 3);
        assert!(p.detail.contains("2 issues imported"), "{}", p.detail);
        assert!(p.detail.contains("1 not available"), "{}", p.detail);
    }

    #[test]
    fn twic_worker_auto_mode_stops_at_first_404() {
        let (_dir, conn) = temp_db();
        let fetcher = MapFetcher::default(); // everything 404s
        let progress = Mutex::new(Some(NetProgress {
            kind: "twic-auto".into(),
            label: "TWIC auto-sync".into(),
            done: 0,
            total: 5,
            detail: String::new(),
            active: true,
            error: None,
        }));
        let stop = AtomicBool::new(false);
        twic_worker_impl(
            &conn,
            &fetcher,
            &[1501, 1502, 1503, 1504, 1505],
            true,
            &progress,
            &stop,
        )
        .unwrap();
        assert_eq!(
            fetcher.log.borrow().len(),
            1,
            "auto mode stops at the first 404 (caught up)"
        );
    }

    #[test]
    fn twic_worker_honors_the_cooperative_stop_flag() {
        let (_dir, conn) = temp_db();
        let fetcher = MapFetcher::default();
        let progress = Mutex::new(Some(NetProgress {
            kind: "twic".into(),
            label: "TWIC download".into(),
            done: 0,
            total: 2,
            detail: String::new(),
            active: true,
            error: None,
        }));
        let stop = AtomicBool::new(true); // cancelled before the first issue
        twic_worker_impl(&conn, &fetcher, &[1500, 1501], false, &progress, &stop).unwrap();
        assert!(fetcher.log.borrow().is_empty(), "no request after cancel");
        let p = progress.lock().unwrap().clone().unwrap();
        assert!(p.detail.contains("cancelled"), "{}", p.detail);
    }

    #[test]
    fn auto_sync_issues_requires_toggle_and_a_resume_point() {
        let (_dir, conn) = temp_db();
        // Toggle off -> None.
        assert_eq!(auto_sync_issues(&conn).unwrap(), None);
        // Toggle on but nothing imported -> None (never guess a start).
        net::meta_set(&conn, META_AUTO_SYNC, "1").unwrap();
        assert_eq!(auto_sync_issues(&conn).unwrap(), None);
        // Imported -> the next `max_issues` (kibitz-db default cap of 5).
        plant_issue(&conn, 1600, 10);
        assert_eq!(
            auto_sync_issues(&conn).unwrap(),
            Some(vec![1601, 1602, 1603, 1604, 1605])
        );
    }

    #[test]
    fn accounts_round_trip_usernames_and_reports() {
        let (_dir, conn) = temp_db();
        let a = sync_accounts_impl(&conn).unwrap();
        assert_eq!(a.lichess.username, None);
        assert_eq!(a.lichess.last_report, None);

        sync_set_username_impl(&conn, "lichess", "  SomeUser  ").unwrap();
        sync_set_username_impl(&conn, "fics", "FicsUser").unwrap();
        assert!(sync_set_username_impl(&conn, "icc", "x").is_err());

        net::meta_set(
            &conn,
            &report_key("lichess"),
            r#"{"at":"2026-07-27 12:00:00","gamesImported":5,"duplicatesSkipped":1,"gamesFailed":0}"#,
        )
        .unwrap();

        let a = sync_accounts_impl(&conn).unwrap();
        assert_eq!(a.lichess.username.as_deref(), Some("SomeUser"), "trimmed");
        assert_eq!(a.lichess.last_report.as_ref().unwrap()["gamesImported"], 5);
        assert_eq!(a.fics.username.as_deref(), Some("FicsUser"));
        assert_eq!(a.chesscom.username, None);

        // Clearing with an empty string removes the key.
        sync_set_username_impl(&conn, "lichess", "").unwrap();
        let a = sync_accounts_impl(&conn).unwrap();
        assert_eq!(a.lichess.username, None);
    }

    #[test]
    fn badges_count_twic_week_and_configured_accounts() {
        let (_dir, conn) = temp_db();
        let b = rail_net_badges_impl(&conn).unwrap();
        assert_eq!(b.twic_latest_imported, None);
        assert_eq!(b.accounts_configured, 0);

        plant_issue(&conn, 1650, 3);
        sync_set_username_impl(&conn, "lichess", "a").unwrap();
        sync_set_username_impl(&conn, "chesscom", "b").unwrap();
        let b = rail_net_badges_impl(&conn).unwrap();
        assert_eq!(b.twic_latest_imported, Some(1650));
        assert_eq!(b.accounts_configured, 2);
    }

    #[test]
    fn summarize_report_covers_the_service_variants() {
        let s = summarize_report(&serde_json::json!({
            "gamesImported": 12, "duplicatesSkipped": 3, "gamesFailed": 0
        }));
        assert_eq!(s, "12 imported · 3 duplicates · 0 failed");
        let s = summarize_report(&serde_json::json!({
            "gamesImported": 2, "duplicatesSkipped": 0, "gamesFailed": 0, "monthsFetched": 4
        }));
        assert!(s.contains("4 month(s) fetched"), "{s}");
        let s = summarize_report(&serde_json::json!({
            "gamesImported": 0, "duplicatesSkipped": 0, "gamesFailed": 0,
            "savedArchive": "/tmp/x.pgn.bz2"
        }));
        assert!(s.contains("bunzip2"), "{s}");
    }
}
