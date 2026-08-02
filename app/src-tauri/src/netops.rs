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

/// Meta key holding a service's auto-sync schedule: "off", "launch", or
/// an interval in hours ("6", "24"). Absent = off.
fn auto_key(service: &str) -> String {
    format!("sync_auto_{service}")
}

/// A service's schedule, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Schedule {
    Off,
    /// Once per app launch, at database open.
    Launch,
    /// At launch and then every N hours WHILE KIBITZ IS RUNNING. A
    /// desktop app has no background daemon: promising "every 6 hours" of
    /// a closed app would be a lie, so the UI says "while running" and
    /// this is what that means.
    EveryHours(u32),
}

impl Schedule {
    pub(crate) fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            None | Some("") | Some("off") => Schedule::Off,
            Some("launch") => Schedule::Launch,
            Some(n) => n
                .parse::<u32>()
                .ok()
                .filter(|h| *h > 0)
                .map(Schedule::EveryHours)
                .unwrap_or(Schedule::Off),
        }
    }

    pub(crate) fn as_str(self) -> String {
        match self {
            Schedule::Off => "off".to_string(),
            Schedule::Launch => "launch".to_string(),
            Schedule::EveryHours(h) => h.to_string(),
        }
    }
}

/// Is `service` due for an automatic sync? `ran_this_launch` is the
/// per-launch latch (a webview reload must not re-arm it), `last_success`
/// the last completed run, `now` the current UTC timestamp.
///
/// Never having synced is always due: that is the case where a username
/// is configured and nothing has ever happened, which is exactly the
/// state Lichess was in.
pub(crate) fn sync_due(
    schedule: Schedule,
    ran_this_launch: bool,
    last_success: Option<&str>,
    now: &str,
) -> bool {
    match schedule {
        Schedule::Off => false,
        Schedule::Launch => !ran_this_launch,
        Schedule::EveryHours(h) => {
            let Some(last) = last_success else {
                return true;
            };
            match hours_between(last, now) {
                Some(elapsed) => elapsed >= f64::from(h),
                // An unparseable timestamp must not wedge the schedule
                // permanently; treat it as due and let the run rewrite it.
                None => true,
            }
        }
    }
}

/// Hours between two "YYYY-MM-DD HH:MM:SS" UTC stamps, or None if either
/// is malformed. Calendar-free: these are the app's own `datetime('now')`
/// values, always UTC and always this format.
fn hours_between(from: &str, to: &str) -> Option<f64> {
    let secs = |s: &str| -> Option<i64> {
        let (date, time) = s.trim().split_once(' ')?;
        let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let t: Vec<i64> = time.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        if d.len() != 3 || t.len() != 3 {
            return None;
        }
        // Days since a fixed epoch via the civil-from-days inverse; only
        // differences are ever used, so the epoch choice is immaterial.
        let (y, m, day) = (d[0], d[1], d[2]);
        let y2 = if m <= 2 { y - 1 } else { y };
        let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
        let yoe = y2 - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe;
        Some(days * 86_400 + t[0] * 3_600 + t[1] * 60 + t[2])
    };
    let a = secs(from)?;
    let b = secs(to)?;
    Some((b - a) as f64 / 3_600.0)
}

/// Meta key: "1" when TWIC auto-download of new issues is enabled.
const META_AUTO_SYNC: &str = "twic_auto_sync";
/// Meta key: "1" once the user acknowledged the TWIC first-run notice.
const META_NOTICE_ACK: &str = "twic_notice_ack";
/// Meta key: newest TWIC issue confirmed published by a catalog probe.
const META_LATEST_KNOWN: &str = "twic_latest_known";
/// Persisted outcome of the last auto-sync pass (JSON {at, text}) — the
/// TWIC screen's idle trace. Silence is indistinguishable from broken
/// (field report 2026-07-28), so every pass leaves a visible record.
const META_AUTO_LAST: &str = "twic_auto_last";

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
    /// Per-LAUNCH latch for TWIC auto-sync. The webview reloads freely
    /// (deep links, dev HMR, StrictMode remounts) and every reload re-runs
    /// the database-open hook; without this backend latch each reload
    /// re-armed the "max 5 per launch" allowance and one session imported
    /// 13+ issues (audit #3). Lives in the app process, not the webview.
    pub auto_sync_ran: Arc<AtomicBool>,
    /// Per-LAUNCH latch for the account schedules, one entry per service
    /// that has already had an automatic run. Same reason as the TWIC
    /// flag above: a webview reload must not re-arm "once per launch".
    pub auto_ran: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Waiting jobs (label + work), drained strictly serially by the one
    /// worker thread — pressing Sync during a TWIC download QUEUES it
    /// instead of rejecting the click (run-9 field report).
    #[allow(clippy::type_complexity)]
    pub queue: Arc<Mutex<std::collections::VecDeque<QueuedNetJob>>>,
}

/// One queued network job: its initial progress snapshot and the work.
pub struct QueuedNetJob {
    pub initial: NetProgress,
    #[allow(clippy::type_complexity)]
    pub job: Box<dyn FnOnce(&AtomicBool, &Mutex<Option<NetProgress>>) -> Result<(), String> + Send>,
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
    /// Labels of jobs waiting behind the current one, in order.
    #[serde(default)]
    pub queued: Vec<String>,
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
pub(crate) fn open_db_path(state: &State<'_, DbState>) -> Result<String, String> {
    with_conn(state, |conn| {
        conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    })
}

pub(crate) fn worker_conn(db_path: &str) -> Result<Connection, String> {
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
    /// Outcome of the last auto-sync pass, if one has ever run.
    pub auto_last: Option<AutoSyncOutcome>,
}

/// What the last auto-sync pass did, persisted in meta so the screen can
/// say so while idle ("up to date", "imported TWIC 1654–1655", an error).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSyncOutcome {
    /// UTC "YYYY-MM-DD HH:MM:SS" (rendered local by the frontend).
    pub at: String,
    pub text: String,
}

fn auto_last_set(conn: &Connection, text: &str) {
    // Best-effort trace: a failed write must never fail the sync itself.
    let outcome = AutoSyncOutcome {
        at: conn
            .query_row("SELECT datetime('now')", [], |r| r.get::<_, String>(0))
            .unwrap_or_default(),
        text: text.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&outcome) {
        let _ = net::meta_set(conn, META_AUTO_LAST, &json);
    }
}

/// Days since the Unix epoch, for the weekly-arithmetic issue estimate.
fn today_epoch_days() -> Result<i64, String> {
    Ok((std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        / 86_400) as i64)
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
        auto_last: net::meta_get(conn, META_AUTO_LAST)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok()),
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
    let guess = twic::estimated_issue(today_epoch_days()?);

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
/// cooperative `stop` flag between issues. A 404 issue is reported and
/// the rest continue (auto-sync runs newest first from a probe-confirmed
/// frontier, so "caught up" is decided before this runs, not by a 404).
fn twic_worker_impl(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    issues: &[u32],
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

/// Enqueue a network job. Strictly serial: if the worker is idle the job
/// starts immediately; otherwise it waits its turn (the click is never
/// discarded). The current progress snapshot advertises the waiting
/// labels so every surface can say "queued behind …".
/// `pub(crate)` so lichess_play.rs can enqueue its finished-game import
/// on the SAME serial worker (one import path, one refresh path).
pub(crate) fn spawn_net_worker(
    worker: &State<'_, NetWorker>,
    initial: NetProgress,
    job: impl FnOnce(&AtomicBool, &Mutex<Option<NetProgress>>) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    {
        let mut queue = worker.queue.lock().expect("net queue poisoned");
        queue.push_back(QueuedNetJob {
            initial,
            job: Box::new(job),
        });
    }
    sync_queued_labels(worker);
    if worker.active.swap(true, Ordering::SeqCst) {
        return Ok(()); // the running drain loop will pick it up
    }
    worker.stop.store(false, Ordering::SeqCst);
    let active = Arc::clone(&worker.active);
    let stop = Arc::clone(&worker.stop);
    let progress = Arc::clone(&worker.progress);
    let queue = Arc::clone(&worker.queue);
    std::thread::spawn(move || {
        loop {
            let next = queue.lock().expect("net queue poisoned").pop_front();
            let Some(QueuedNetJob { mut initial, job }) = next else {
                break;
            };
            // A cancel applies to the job it was pressed on, not the queue.
            stop.store(false, Ordering::SeqCst);
            initial.queued = queue
                .lock()
                .expect("net queue poisoned")
                .iter()
                .map(|q| q.initial.label.clone())
                .collect();
            set_progress(&progress, initial);
            // Panic armor (2026-07-28 field report): a panicking job used
            // to kill this thread silently — progress stayed "active"
            // forever and every queued sync wedged behind a ghost until
            // the app restarted. A panic is now an ordinary job failure:
            // visible error, worker moves on.
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&stop, &progress)))
                    .unwrap_or_else(|panic| {
                        let msg = panic
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| panic.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".to_string());
                        Err(format!("internal error (please report): {msg}"))
                    });
            if let Err(e) = &result {
                update_progress(&progress, |p| p.error = Some(e.clone()));
            }
        }
        update_progress(&progress, |p| {
            p.active = false;
            p.queued.clear();
        });
        active.store(false, Ordering::SeqCst);
    });
    Ok(())
}

/// Refresh the advertised queue labels on the current progress snapshot.
fn sync_queued_labels(worker: &State<'_, NetWorker>) {
    let labels: Vec<String> = worker
        .queue
        .lock()
        .expect("net queue poisoned")
        .iter()
        .map(|q| q.initial.label.clone())
        .collect();
    update_progress(&worker.progress, |p| p.queued = labels);
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
        queued: Vec::new(),
        error: None,
    };
    spawn_net_worker(&worker, initial, move |stop, progress| {
        let conn = worker_conn(&db_path)?;
        twic_worker_impl(&conn, &net::UreqFetcher, &todo, progress, stop)
    })?;
    Ok(count)
}

/// Should the database-open hook start an auto-sync? Requires the toggle,
/// a real resume point (we never guess a starting issue), and that this
/// LAUNCH has not already run one (`already_ran` — the [`NetWorker`]
/// latch), so a webview reload can never re-arm the per-launch cap.
pub(crate) fn should_auto_sync(conn: &Connection, already_ran: bool) -> Result<bool, String> {
    if already_ran || !meta_flag(conn, META_AUTO_SYNC) {
        return Ok(false);
    }
    Ok(twic::latest_imported(conn)
        .map_err(|e| e.to_string())?
        .is_some())
}

/// The issues one auto-sync pass downloads: the newest missing issues
/// FIRST (current weeks arrive immediately — audit #3 saw them last),
/// capped at `cap`, and never older than the newest import — backfilling
/// older gaps stays a manual action on the TWIC screen ("Download all
/// missing" / checkbox selection).
pub(crate) fn auto_sync_plan(
    newest_published: u32,
    latest_imported: u32,
    imported: &std::collections::HashSet<u32>,
    cap: usize,
    oldest_imported: Option<u32>,
) -> Vec<u32> {
    // New issues first: the current week is what a user is waiting for.
    let mut plan: Vec<u32> = (latest_imported + 1..=newest_published)
        .rev()
        .filter(|i| !imported.contains(i))
        .take(cap)
        .collect();
    if plan.len() >= cap {
        return plan;
    }
    // Then spend what is left of the cap filling gaps BEHIND the newest
    // import, newest first (2026-08-02 field report: "what about all the
    // old ones, I don't have all of them downloaded yet — why isn't it
    // synchronizing them?"). Auto-sync used to stop at the first line
    // above, so a database with 91 of 1655 issues stayed at 91 forever
    // while reporting itself up to date, which is true only of the front
    // edge and useless to someone with 645 holes behind it.
    //
    // Still capped per pass: TWIC's hosting is donation-funded, and the
    // point is to converge over many runs, not to pull a decade of
    // archives in one. `oldest_imported` bounds the walk so the backfill
    // stops at the user's own archive rather than marching to issue 1.
    let floor = oldest_imported.unwrap_or(latest_imported);
    if floor < latest_imported {
        let older = (floor..latest_imported)
            .rev()
            .filter(|i| !imported.contains(i))
            .take(cap - plan.len());
        plan.extend(older);
    }
    plan
}

/// One auto-sync pass on the worker thread: probe the newest published
/// issue (weekly-arithmetic guess, floor = newest already known), record
/// it in meta, then import per [`auto_sync_plan`] — newest first.
fn twic_auto_worker_impl(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    guess: u32,
    cap: usize,
    progress: &Mutex<Option<NetProgress>>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let latest_imported = twic::latest_imported(conn)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "auto-sync needs at least one imported issue to resume from".to_string())?;
    let floor = latest_imported.max(meta_u32(conn, META_LATEST_KNOWN).unwrap_or(0));
    let probe = twic::probe_latest(fetcher, Some(floor), guess, &mut std::thread::sleep)
        .map_err(|e| format!("{e:#}"))?;
    let Some(newest) = probe.latest else {
        update_progress(progress, |p| {
            p.detail = "no published issue found".to_string();
        });
        auto_last_set(conn, "checked — no published issue found");
        return Ok(());
    };
    net::meta_set(conn, META_LATEST_KNOWN, &newest.to_string()).map_err(|e| e.to_string())?;

    let imported: std::collections::HashSet<u32> = twic::imported_issues(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(issue, _)| issue)
        .collect();
    let oldest_imported = imported.iter().copied().min();
    let plan = auto_sync_plan(newest, latest_imported, &imported, cap, oldest_imported);
    if plan.is_empty() {
        update_progress(progress, |p| {
            p.detail = format!("up to date — TWIC {newest} is the newest published issue");
        });
        auto_last_set(
            conn,
            &format!("up to date — TWIC {newest} is the newest published issue"),
        );
        return Ok(());
    }
    update_progress(progress, |p| {
        p.total = plan.len() as u32;
        p.detail = format!(
            "{} new issue{}, newest first…",
            plan.len(),
            if plan.len() == 1 { "" } else { "s" }
        );
    });
    // Explicit-selection mode (stop_at_404 = false): the plan runs newest
    // first, so a 404 on one issue must not abandon the older ones.
    let result = twic_worker_impl(conn, fetcher, &plan, progress, stop);
    let lo = plan.iter().min().copied().unwrap_or(newest);
    let hi = plan.iter().max().copied().unwrap_or(newest);
    let span = if lo == hi {
        format!("TWIC {hi}")
    } else {
        format!("TWIC {lo}–{hi}")
    };
    match &result {
        Ok(()) => auto_last_set(conn, &format!("imported {span} (newest first)")),
        Err(e) => auto_last_set(conn, &format!("failed on {span}: {e}")),
    }
    result
}

/// Database-open hook: when the auto-download toggle is on, quietly fetch
/// the NEWEST missing issues (newest first, per-launch cap = the
/// kibitz-db default of 5, strictly serial). The cap is enforced by a
/// backend latch — at most one auto-sync pass per app launch, no matter
/// how often the webview reloads and re-fires this hook. Returns true
/// when a sync was started.
#[tauri::command]
pub async fn twic_auto_sync_check(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
) -> Result<bool, String> {
    let already_ran = worker.auto_sync_ran.load(Ordering::SeqCst);
    if !with_conn(&state, |conn| should_auto_sync(conn, already_ran))? {
        return Ok(false);
    }
    if worker.active.load(Ordering::SeqCst) {
        return Ok(false); // never queue behind another network job
    }
    if worker.auto_sync_ran.swap(true, Ordering::SeqCst) {
        return Ok(false); // another check won the race this launch
    }
    let db_path = open_db_path(&state)?;
    let guess = twic::estimated_issue(today_epoch_days()?);
    let cap = twic::TwicOptions::default().max_issues as usize;
    let initial = NetProgress {
        kind: "twic-auto".to_string(),
        label: "TWIC auto-sync".to_string(),
        done: 0,
        total: 0, // known after the probe
        detail: "checking the newest published issue…".to_string(),
        active: true,
        queued: Vec::new(),
        error: None,
    };
    spawn_net_worker(&worker, initial, move |stop, progress| {
        let conn = worker_conn(&db_path)?;
        // Log the pass whatever it finds. "Checked, nothing new" is the
        // evidence the schedule is alive; without a row it is
        // indistinguishable from never having run.
        let started = now_utc(&conn);
        let run = net::sync_run_start(&conn, "twic", "auto", &started).ok();
        let result = twic_auto_worker_impl(&conn, &net::UreqFetcher, guess, cap, progress, stop);
        if let Some(id) = run {
            let at = now_utc(&conn);
            let detail = net::meta_get(&conn, META_AUTO_LAST)
                .ok()
                .flatten()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .and_then(|v| v["text"].as_str().map(str::to_string));
            let err = result.as_ref().err().cloned();
            let _ =
                net::sync_run_finish(&conn, id, &at, 0, 0, 0, detail.as_deref(), err.as_deref());
        }
        result
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
    /// Games in the database from this service, counted live from the
    /// provenance rows (`sources`) — the idle card's "N games imported
    /// total" (audit #16/#21). Survives report resets and counts imports
    /// from every run, not just the last one.
    pub games_total: i64,
}

/// The `sources.name` prefix each service's client stamps on its imports
/// (see kibitz-db::net::{lichess,chesscom,fics}) — the provenance handle
/// for per-service totals.
fn service_source_prefix(service: &str) -> &'static str {
    match service {
        "lichess" => "Lichess: ",
        "chesscom" => "chess.com: ",
        _ => "FICS: ",
    }
}

fn service_games_total(conn: &Connection, service: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM games g JOIN sources s ON s.id = g.source_id
         WHERE s.kind = 'online' AND s.name LIKE ?1 || '%'",
        [service_source_prefix(service)],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAccounts {
    pub lichess: ServiceAccount,
    pub chesscom: ServiceAccount,
    pub fics: ServiceAccount,
}

fn service_account(conn: &Connection, service: &str) -> Result<ServiceAccount, String> {
    let username = net::meta_get(conn, &user_key(service))
        .ok()
        .flatten()
        .filter(|u| !u.is_empty());
    let last_report = net::meta_get(conn, &report_key(service))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok());
    Ok(ServiceAccount {
        username,
        last_report,
        games_total: service_games_total(conn, service)?,
    })
}

pub(crate) fn sync_accounts_impl(conn: &Connection) -> Result<SyncAccounts, String> {
    Ok(SyncAccounts {
        lichess: service_account(conn, "lichess")?,
        chesscom: service_account(conn, "chesscom")?,
        fics: service_account(conn, "fics")?,
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

/// Read/write a service's auto-sync schedule.
#[tauri::command]
pub async fn sync_schedule_get(state: State<'_, DbState>, service: String) -> Result<String, String> {
    check_service(&service)?;
    with_conn(&state, |conn| {
        Ok(Schedule::parse(
            net::meta_get(conn, &auto_key(&service))
                .map_err(|e| e.to_string())?
                .as_deref(),
        )
        .as_str())
    })
}

#[tauri::command]
pub async fn sync_schedule_set(
    state: State<'_, DbState>,
    service: String,
    schedule: String,
) -> Result<String, String> {
    check_service(&service)?;
    let parsed = Schedule::parse(Some(&schedule));
    with_conn(&state, |conn| {
        net::meta_set(conn, &auto_key(&service), &parsed.as_str()).map_err(|e| e.to_string())?;
        Ok(parsed.as_str())
    })
}

/// Services that are due right now: a username is configured, a schedule
/// says so, and the worker is free. The caller runs them one at a time —
/// the network worker is strictly serial and a queue of syncs behind a
/// user's manual action would be worse than not running at all.
#[tauri::command]
pub async fn sync_due_now(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
) -> Result<Vec<String>, String> {
    if worker.active.load(Ordering::SeqCst) {
        return Ok(Vec::new());
    }
    let ran: std::collections::HashSet<String> = worker
        .auto_ran
        .lock()
        .map(|r| r.clone())
        .unwrap_or_default();
    with_conn(&state, |conn| {
        let now = now_utc(conn);
        due_services(conn, &ran, &now)
    })
}

/// The due-selection itself, over a connection — so the path a user never
/// reaches through the UI is still exercised against a real database.
pub(crate) fn due_services(
    conn: &Connection,
    ran: &std::collections::HashSet<String>,
    now: &str,
) -> Result<Vec<String>, String> {
    let mut due = Vec::new();
    for service in ["lichess", "chesscom"] {
        let schedule = Schedule::parse(
            net::meta_get(conn, &auto_key(service))
                .map_err(|e| e.to_string())?
                .as_deref(),
        );
        let username = net::meta_get(conn, &format!("sync_user_{service}"))
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        // A schedule without a username is not a sync waiting to happen,
        // it is an unconfigured account.
        if username.trim().is_empty() {
            continue;
        }
        let last = net::last_successful_sync(conn, service).map_err(|e| e.to_string())?;
        if sync_due(schedule, ran.contains(service), last.as_deref(), now) {
            due.push(service.to_string());
        }
    }
    Ok(due)
}

/// Recent sync attempts, newest first — the answer to "has it actually
/// been downloading anything?". Includes automatic passes that found
/// nothing, which is the case a last-outcome field cannot express.
#[tauri::command]
pub async fn sync_history(
    state: State<'_, DbState>,
    service: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<net::SyncRun>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 200);
    with_conn(&state, |conn| {
        net::sync_runs(conn, service.as_deref(), limit).map_err(|e| e.to_string())
    })
}

/// Run one account sync on the background worker using the existing
/// kibitz-db client (strictly serial; the clients resume incrementally
/// from their own per-username meta cursors). FICS requires `year`
/// (optionally `month`) — there is no incremental cursor for it. The
/// result (or the error) is persisted as the service's last report, and
/// the run is recorded in `sync_runs`. `trigger` is "auto" when a
/// schedule started it; anything else is a person pressing the button —
/// the distinction the log exists to make.
#[tauri::command]
pub async fn sync_run(
    state: State<'_, DbState>,
    worker: State<'_, NetWorker>,
    service: String,
    username: String,
    year: Option<u16>,
    month: Option<u8>,
    trigger: Option<String>,
) -> Result<(), String> {
    let trigger = if trigger.as_deref() == Some("auto") {
        "auto".to_string()
    } else {
        "manual".to_string()
    };
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
    if trigger == "auto" {
        // Latch BEFORE spawning: a "once per launch" schedule must not
        // re-arm because two ticks raced, and a failed run still counts as
        // this launch's attempt (the interval schedules retry on their own
        // clock, which last_successful_sync anchors).
        if let Ok(mut ran) = worker.auto_ran.lock() {
            ran.insert(service.clone());
        }
    }
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
        queued: Vec::new(),
        error: None,
    };
    spawn_net_worker(&worker, initial, move |stop, progress| {
        let conn = worker_conn(&db_path)?;
        let fetcher = net::UreqFetcher;
        let started = now_utc(&conn);
        let run = net::sync_run_start(&conn, &service, &trigger, &started).ok();
        let outcome = run_service_sync(
            &conn, &fetcher, &service, &username, year, month, stop, progress,
        );
        let at = now_utc(&conn);
        if let Some(id) = run {
            let num = |v: &serde_json::Value, k: &str| v[k].as_i64().unwrap_or(0);
            let (imported, dups, failed, err) = match &outcome {
                Ok(r) => (
                    num(r, "gamesImported"),
                    num(r, "duplicatesSkipped"),
                    num(r, "gamesFailed"),
                    None,
                ),
                Err(e) => (0, 0, 0, Some(e.clone())),
            };
            let _ =
                net::sync_run_finish(&conn, id, &at, imported, dups, failed, None, err.as_deref());
        }
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
#[allow(clippy::too_many_arguments)] // one call site; a params struct adds noise
fn run_service_sync(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    service: &str,
    username: &str,
    year: Option<u16>,
    month: Option<u8>,
    stop: &AtomicBool,
    progress: &Mutex<Option<NetProgress>>,
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
            // Month-by-month sync HAS an honest fraction — show it, and
            // honor Cancel between months (the cursor stays correct, the
            // next run resumes at the first unfinished month). Months are
            // PACED: chess.com throttles bursts of month downloads (the
            // 2026-07-28 wedge), so give the API breathing room.
            let mut first_month = true;
            let r = kibitz_db::net::chesscom::sync_user_observed(
                conn,
                fetcher,
                username,
                &mut |done, total, current, games| {
                    if !first_month {
                        std::thread::sleep(std::time::Duration::from_millis(600));
                    }
                    first_month = false;
                    update_progress(progress, |p| {
                        p.done = done as u32;
                        p.total = total as u32;
                        p.detail = format!(
                            "{current} · {games} games imported so far · resumes automatically                              if interrupted"
                        );
                    });
                    !stop.load(Ordering::SeqCst)
                },
            )
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
            queued: Vec::new(),
            error: None,
        }));
        let stop = AtomicBool::new(false);
        twic_worker_impl(&conn, &fetcher, &[1500, 1501, 1502], &progress, &stop).unwrap();

        assert_eq!(
            fetcher.log.borrow().as_slice(),
            &[
                twic::zip_url(1500),
                twic::zip_url(1501),
                twic::zip_url(1502)
            ],
            "strictly serial, in the given order"
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
            queued: Vec::new(),
            error: None,
        }));
        let stop = AtomicBool::new(true); // cancelled before the first issue
        twic_worker_impl(&conn, &fetcher, &[1500, 1501], &progress, &stop).unwrap();
        assert!(fetcher.log.borrow().is_empty(), "no request after cancel");
        let p = progress.lock().unwrap().clone().unwrap();
        assert!(p.detail.contains("cancelled"), "{}", p.detail);
    }

    #[test]
    fn auto_sync_gate_requires_toggle_resume_point_and_first_run_this_launch() {
        let (_dir, conn) = temp_db();
        // Toggle off -> no.
        assert!(!should_auto_sync(&conn, false).unwrap());
        // Toggle on but nothing imported -> no (never guess a start).
        net::meta_set(&conn, META_AUTO_SYNC, "1").unwrap();
        assert!(!should_auto_sync(&conn, false).unwrap());
        // Imported -> yes, but ONLY once per launch: the second database-
        // open hook of the same app process (webview reload, StrictMode
        // remount) must not re-arm the cap (audit #3).
        plant_issue(&conn, 1600, 10);
        assert!(should_auto_sync(&conn, false).unwrap());
        assert!(!should_auto_sync(&conn, true).unwrap());
    }

    #[test]
    fn auto_sync_plan_puts_new_issues_first_and_caps_the_pass() {
        use std::collections::HashSet;
        let imported: HashSet<u32> = [1600].into_iter().collect();
        // 10 unpublished-locally issues, cap 5: the NEWEST five, descending.
        assert_eq!(
            auto_sync_plan(1610, 1600, &imported, 5, None),
            vec![1610, 1609, 1608, 1607, 1606]
        );
        // Issues at or below the newest import are backfill — manual only.
        let imported: HashSet<u32> = [1595, 1600].into_iter().collect();
        assert_eq!(
            auto_sync_plan(1602, 1600, &imported, 5, None),
            vec![1602, 1601]
        );
        // Already-imported issues inside the window are skipped, cap holds.
        let imported: HashSet<u32> = [1600, 1609].into_iter().collect();
        assert_eq!(
            auto_sync_plan(1610, 1600, &imported, 5, None),
            vec![1610, 1608, 1607, 1606, 1605]
        );
        // Fully caught up -> empty plan.
        let imported: HashSet<u32> = [1610].into_iter().collect();
        assert_eq!(
            auto_sync_plan(1610, 1610, &imported, 5, None),
            Vec::<u32>::new()
        );
    }

    /// The whole loop against a real database, because the scheduler is
    /// otherwise a path no user can reach yet: schedule set -> due -> run
    /// recorded -> not due -> interval elapses -> due again.
    #[test]
    fn the_schedule_loop_runs_end_to_end_on_a_real_database() {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let none = std::collections::HashSet::new();

        // A configured username with no schedule is not due.
        sync_set_username_impl(&conn, "lichess", "avienu").unwrap();
        assert!(due_services(&conn, &none, "2026-08-02 12:00:00")
            .unwrap()
            .is_empty());

        // Scheduled and NEVER synced: due. This is the branch that would
        // otherwise be true in principle and unreached in practice — the
        // real Lichess account has been in exactly this state.
        kibitz_db::net::meta_set(&conn, &auto_key("lichess"), "6").unwrap();
        assert_eq!(
            due_services(&conn, &none, "2026-08-02 12:00:00").unwrap(),
            vec!["lichess".to_string()]
        );

        // A run lands in the log; the interval is now measured from it.
        let id =
            kibitz_db::net::sync_run_start(&conn, "lichess", "auto", "2026-08-02 12:00:00").unwrap();
        kibitz_db::net::sync_run_finish(&conn, id, "2026-08-02 12:00:30", 7, 0, 0, None, None)
            .unwrap();
        let logged = kibitz_db::net::sync_runs(&conn, Some("lichess"), 5).unwrap();
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].trigger, "auto");
        assert_eq!(logged[0].games_imported, 7);

        // Five hours later: not due. Six: due.
        assert!(due_services(&conn, &none, "2026-08-02 17:00:00")
            .unwrap()
            .is_empty());
        assert_eq!(
            due_services(&conn, &none, "2026-08-02 18:00:30").unwrap(),
            vec!["lichess".to_string()]
        );

        // A FAILED run does not reset the clock — still due.
        let bad =
            kibitz_db::net::sync_run_start(&conn, "lichess", "auto", "2026-08-02 18:00:31").unwrap();
        kibitz_db::net::sync_run_finish(
            &conn,
            bad,
            "2026-08-02 18:00:32",
            0,
            0,
            0,
            None,
            Some("429"),
        )
        .unwrap();
        assert_eq!(
            due_services(&conn, &none, "2026-08-02 18:01:00").unwrap(),
            vec!["lichess".to_string()],
            "a failure must not look like a completed check"
        );

        // A username-less service with a schedule stays out of the list.
        kibitz_db::net::meta_set(&conn, &auto_key("chesscom"), "launch").unwrap();
        assert_eq!(
            due_services(&conn, &none, "2026-08-02 18:01:00").unwrap(),
            vec!["lichess".to_string()]
        );

        // The per-launch latch removes a "launch" service once it has run.
        sync_set_username_impl(&conn, "chesscom", "sounix").unwrap();
        let mut ran = std::collections::HashSet::new();
        assert!(due_services(&conn, &ran, "2026-08-02 18:01:00")
            .unwrap()
            .contains(&"chesscom".to_string()));
        ran.insert("chesscom".to_string());
        assert!(!due_services(&conn, &ran, "2026-08-02 18:01:00")
            .unwrap()
            .contains(&"chesscom".to_string()));
    }

    #[test]
    fn schedule_round_trips_and_rejects_nonsense() {
        assert_eq!(Schedule::parse(None), Schedule::Off);
        assert_eq!(Schedule::parse(Some("")), Schedule::Off);
        assert_eq!(Schedule::parse(Some("off")), Schedule::Off);
        assert_eq!(Schedule::parse(Some("launch")), Schedule::Launch);
        assert_eq!(Schedule::parse(Some("6")), Schedule::EveryHours(6));
        // Zero hours would be a busy loop against someone else's server.
        assert_eq!(Schedule::parse(Some("0")), Schedule::Off);
        assert_eq!(Schedule::parse(Some("banana")), Schedule::Off);
        for s in [Schedule::Off, Schedule::Launch, Schedule::EveryHours(24)] {
            assert_eq!(Schedule::parse(Some(&s.as_str())), s);
        }
    }

    #[test]
    fn sync_due_respects_the_launch_latch_and_the_interval() {
        let now = "2026-08-02 12:00:00";
        // Off is off, whatever the history says.
        assert!(!sync_due(Schedule::Off, false, None, now));

        // Launch: once, then not again until the next launch.
        assert!(sync_due(Schedule::Launch, false, Some("2026-08-02 11:59:00"), now));
        assert!(!sync_due(Schedule::Launch, true, None, now));

        // Interval counts from the last SUCCESS.
        assert!(!sync_due(
            Schedule::EveryHours(6),
            true,
            Some("2026-08-02 08:00:00"),
            now
        ));
        assert!(sync_due(
            Schedule::EveryHours(6),
            true,
            Some("2026-08-02 05:59:00"),
            now
        ));
        // Exactly on the boundary is due.
        assert!(sync_due(
            Schedule::EveryHours(6),
            true,
            Some("2026-08-02 06:00:00"),
            now
        ));

        // Never synced is always due — the Lichess case: a username
        // configured and nothing has ever run.
        assert!(sync_due(Schedule::EveryHours(24), true, None, now));

        // A corrupt timestamp must not wedge the schedule forever.
        assert!(sync_due(Schedule::EveryHours(6), true, Some("not a date"), now));
    }

    #[test]
    fn hours_between_spans_days_months_and_years() {
        let h = |a: &str, b: &str| hours_between(a, b).unwrap();
        assert!((h("2026-08-02 00:00:00", "2026-08-02 06:30:00") - 6.5).abs() < 1e-9);
        assert!((h("2026-07-31 23:00:00", "2026-08-01 01:00:00") - 2.0).abs() < 1e-9);
        assert!((h("2025-12-31 23:00:00", "2026-01-01 00:00:00") - 1.0).abs() < 1e-9);
        // Leap day, and the maintainer's real gap: 2026-07-29 -> 2026-08-02.
        assert!((h("2024-02-28 12:00:00", "2024-03-01 12:00:00") - 48.0).abs() < 1e-9);
        assert!((h("2026-07-29 04:01:08", "2026-08-02 04:01:08") - 96.0).abs() < 1e-9);
        assert!(hours_between("bad", "2026-08-02 00:00:00").is_none());
    }

    /// A database with holes behind its newest issue used to sit there
    /// reporting itself up to date (2026-08-02: 91 issues of 1655, 645
    /// gaps, auto-sync fetching nothing).
    #[test]
    fn auto_sync_backfills_gaps_behind_the_newest_import() {
        let imported: std::collections::HashSet<u32> = [1600, 1598, 1500].into_iter().collect();

        // Nothing new published: the whole cap goes to the gaps, newest
        // first, and never below the user's own oldest issue.
        let plan = auto_sync_plan(1600, 1600, &imported, 4, Some(1500));
        assert_eq!(plan, vec![1599, 1597, 1596, 1595]);

        // New issues still come FIRST and are never displaced by backfill.
        let plan = auto_sync_plan(1603, 1600, &imported, 4, Some(1500));
        assert_eq!(plan, vec![1603, 1602, 1601, 1599]);

        // A full cap of new issues leaves nothing for the backfill.
        let plan = auto_sync_plan(1610, 1600, &imported, 3, Some(1500));
        assert_eq!(plan, vec![1610, 1609, 1608]);

        // A complete archive asks for nothing.
        let full: std::collections::HashSet<u32> = (1500..=1600).collect();
        assert!(auto_sync_plan(1600, 1600, &full, 5, Some(1500)).is_empty());

        // Without an oldest issue there is no floor to walk back to, so
        // the behaviour is exactly what it was before.
        assert!(auto_sync_plan(1600, 1600, &imported, 5, None).is_empty());
    }

    #[test]
    fn auto_sync_worker_probes_then_downloads_newest_first_within_the_cap() {
        let (_dir, conn) = temp_db();
        plant_issue(&conn, 1500, 10);
        let mut fetcher = MapFetcher::default();
        for issue in 1501..=1510 {
            fetcher.bodies.insert(twic::zip_url(issue), ZIP_A.to_vec());
        }
        let progress = Mutex::new(Some(NetProgress {
            kind: "twic-auto".into(),
            label: "TWIC auto-sync".into(),
            done: 0,
            total: 0,
            detail: String::new(),
            active: true,
            queued: Vec::new(),
            error: None,
        }));
        let stop = AtomicBool::new(false);
        twic_auto_worker_impl(&conn, &fetcher, 1505, 5, &progress, &stop).unwrap();

        // Probe from the guess (1505): forward until 1511 404s, then the
        // download plan runs NEWEST FIRST — 1510 down to 1506, cap 5.
        let expected: Vec<String> = (1505..=1511)
            .map(twic::zip_url)
            .chain([1510, 1509, 1508, 1507, 1506].map(twic::zip_url))
            .collect();
        assert_eq!(fetcher.log.borrow().as_slice(), expected.as_slice());
        assert_eq!(
            twic::imported_issues(&conn)
                .unwrap()
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            vec![1500, 1506, 1507, 1508, 1509, 1510],
            "the five newest issues; 1501–1505 stay manual backfill"
        );
        assert_eq!(
            net::meta_get(&conn, META_LATEST_KNOWN).unwrap().as_deref(),
            Some("1510"),
            "the probe result is recorded for the catalog"
        );
        let p = progress.lock().unwrap().clone().unwrap();
        assert_eq!(p.total, 5, "total set once the plan is known");
        assert_eq!(p.done, 5);

        // The pass leaves a persisted idle trace (field report 2026-07-28:
        // a silent auto-sync reads as broken).
        let last: AutoSyncOutcome =
            serde_json::from_str(&net::meta_get(&conn, META_AUTO_LAST).unwrap().unwrap()).unwrap();
        assert_eq!(last.text, "imported TWIC 1506–1510 (newest first)");
        assert!(!last.at.is_empty());
    }

    #[test]
    fn auto_sync_worker_reports_up_to_date_without_downloading() {
        let (_dir, conn) = temp_db();
        plant_issue(&conn, 1500, 10);
        net::meta_set(&conn, META_LATEST_KNOWN, "1500").unwrap();
        let fetcher = MapFetcher::default(); // nothing newer published
        let progress = Mutex::new(Some(NetProgress {
            kind: "twic-auto".into(),
            label: "TWIC auto-sync".into(),
            done: 0,
            total: 0,
            detail: String::new(),
            active: true,
            queued: Vec::new(),
            error: None,
        }));
        let stop = AtomicBool::new(false);
        twic_auto_worker_impl(&conn, &fetcher, 1500, 5, &progress, &stop).unwrap();
        assert_eq!(twic::imported_issues(&conn).unwrap().len(), 1);
        let p = progress.lock().unwrap().clone().unwrap();
        assert!(p.detail.contains("up to date"), "{}", p.detail);
        // Idle trace persisted, and the catalog serves it to the screen.
        let cat = twic_catalog_impl(&conn).unwrap();
        let last = cat.auto_last.expect("outcome recorded");
        assert!(last.text.contains("up to date"), "{}", last.text);
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
    fn accounts_carry_per_service_totals_from_provenance_rows() {
        use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
        let (_dir, conn) = temp_db();
        let a = sync_accounts_impl(&conn).unwrap();
        assert_eq!(a.lichess.games_total, 0, "empty db: honest zero");

        // One Lichess import (the client's exact source-name shape).
        let source = SourceInfo {
            name: "Lichess: SomeUser".into(),
            origin: "https://lichess.org/api/games/user/SomeUser".into(),
            license: "user's own games".into(),
            kind: SourceKind::Online,
        };
        let pgn = "[White \"SomeUser\"]\n[Black \"Opp\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
        let st = import_pgn(&conn, &source, std::io::Cursor::new(pgn)).unwrap();
        assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);

        let a = sync_accounts_impl(&conn).unwrap();
        assert_eq!(a.lichess.games_total, 1);
        assert_eq!(a.chesscom.games_total, 0, "prefixes do not cross-count");
        assert_eq!(a.fics.games_total, 0);
        // Wire shape: the idle card reads gamesTotal.
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"gamesTotal\":1"), "{json}");
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
