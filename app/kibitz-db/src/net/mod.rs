//! Network clients: Lichess, chess.com, ICC, FICS (app layer only).
//!
//! Shared plumbing for all network ingestion:
//!
//! - the [`Fetcher`] abstraction, so every client is testable fully offline
//!   against fixture bytes (`cargo test` never touches the network);
//! - [`UreqFetcher`], the production implementation backed by `ureq`;
//! - the HTTP 429 backoff policy ([`backoff_delay`] is the pure decision
//!   function, [`fetch_with_retry`] the driver with an injectable sleeper);
//! - `meta`-table helpers used by the incremental sync clients to remember
//!   where they left off.
//!
//! All clients issue strictly serial requests by design — never concurrent —
//! and every request carries the descriptive [`user_agent`].

pub mod chesscom;
pub mod fics;
pub mod icc;
pub mod lichess;
pub mod llm;

use std::io::Read;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

/// User-Agent header sent with every request from every client.
///
/// Contact address per Lichess/chess.com API etiquette.
/// before shipping — both Lichess and chess.com ask API consumers to be
/// identifiable and contactable.
/// Built once: `kibitz/0.1 (chess database; contact: <KIBITZ_CONTACT>)`.
/// The contact address is config-supplied (env `KIBITZ_CONTACT`) and never
/// committed; without one the UA points at the project repository, which
/// is still an identifiable, contactable origin per API etiquette.
pub fn user_agent() -> &'static str {
    static UA: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    UA.get_or_init(|| {
        let contact = std::env::var("KIBITZ_CONTACT")
            .unwrap_or_else(|_| "contact@kibitzchess.org".to_string());
        format!("kibitz/0.1 (chess database; contact: {contact})")
    })
}

/// How many HTTP 429 responses a single logical fetch tolerates (sleeping
/// between attempts per [`backoff_delay`]) before aborting the sync.
pub const MAX_RATE_LIMIT_FAILURES: u32 = 4;

/// Fallback backoff when a 429 response carries no `Retry-After` header.
pub const DEFAULT_BACKOFF_SECS: u64 = 60;

/// Outcome of a single HTTP GET, reduced to the three cases the ingestion
/// clients care about. Any other error (transport failure, unexpected status)
/// surfaces as an `Err` from [`Fetcher::get`].
pub enum FetchOutcome {
    /// 2xx: the (possibly streaming) response body.
    Body(Box<dyn Read + Send>),
    /// 404 — meaningful to clients (e.g. "no newer TWIC issue yet").
    NotFound,
    /// 429 — the caller should back off and retry.
    RateLimited {
        /// Parsed `Retry-After` header (seconds), if the server sent one.
        retry_after_secs: Option<u64>,
    },
}

/// Minimal injectable HTTP abstraction. Production code uses
/// [`UreqFetcher`]; tests supply a fixture implementation, keeping the
/// default test suite fully offline.
pub trait Fetcher {
    /// Perform a GET on `url` with the given extra request headers
    /// (e.g. `Accept`). Implementations must send [`user_agent`].
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> anyhow::Result<FetchOutcome>;

    /// Perform a form-encoded POST (used by the ficsgames.org client).
    /// Implementations must send [`user_agent`].
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> anyhow::Result<FetchOutcome>;

    /// Existence check for `url` (used by the TWIC catalog probe). The
    /// default falls back to [`Fetcher::get`] with the body dropped
    /// unread, so existing implementations — including offline test
    /// fixtures — keep working; [`UreqFetcher`] overrides it with a real
    /// HEAD request so probes never transfer issue bodies.
    fn head(&self, url: &str) -> anyhow::Result<FetchOutcome> {
        self.get(url, &[])
    }
}

/// Production [`Fetcher`] backed by `ureq` (blocking, serial).
pub struct UreqFetcher;

/// Shared agent with real timeouts. The default `ureq` agent has NONE: a
/// server that throttles by stalling the connection (chess.com does this
/// to bursts, mid-response-body) blocks the read forever and wedges the
/// strictly-serial network worker until the app is quit — the 2026-07-28
/// field report's chess.com sync hung at the same month for a full day.
/// With timeouts a stall becomes an error: the sync fails visibly, the
/// report records it, and the next run resumes from the cursor.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            // Per-read, not whole-body: large TWIC zips stream fine as
            // long as bytes keep arriving.
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build()
    })
}

/// Map a `ureq` result to a [`FetchOutcome`].
fn outcome(result: Result<ureq::Response, ureq::Error>, url: &str) -> anyhow::Result<FetchOutcome> {
    match result {
        Ok(resp) => Ok(FetchOutcome::Body(Box::new(resp.into_reader()))),
        Err(ureq::Error::Status(404, _)) => Ok(FetchOutcome::NotFound),
        Err(ureq::Error::Status(429, resp)) => Ok(FetchOutcome::RateLimited {
            retry_after_secs: resp
                .header("Retry-After")
                .and_then(|v| v.trim().parse().ok()),
        }),
        Err(ureq::Error::Status(code, _)) => anyhow::bail!("HTTP {code} for {url}"),
        Err(e) => Err(anyhow::Error::from(e).context(format!("request to {url}"))),
    }
}

impl Fetcher for UreqFetcher {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        let mut req = agent().get(url).set("User-Agent", user_agent());
        for (k, v) in headers {
            req = req.set(k, v);
        }
        outcome(req.call(), url)
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        let req = agent().post(url).set("User-Agent", user_agent());
        outcome(req.send_form(form), url)
    }

    fn head(&self, url: &str) -> anyhow::Result<FetchOutcome> {
        let req = agent().head(url).set("User-Agent", user_agent());
        outcome(req.call(), url)
    }
}

/// Pure 429 backoff decision.
///
/// `failures_so_far` counts 429 responses already received for this logical
/// fetch (including the one being handled). Returns `None` when the fetch
/// should be aborted (`failures_so_far >= max_failures`), otherwise the
/// duration to sleep before retrying: the server's `Retry-After` if present,
/// else [`DEFAULT_BACKOFF_SECS`].
pub fn backoff_delay(
    retry_after_secs: Option<u64>,
    failures_so_far: u32,
    max_failures: u32,
) -> Option<Duration> {
    if failures_so_far >= max_failures {
        None
    } else {
        Some(Duration::from_secs(
            retry_after_secs.unwrap_or(DEFAULT_BACKOFF_SECS),
        ))
    }
}

/// Drive `attempt` (one HTTP request) to completion, honoring 429 backoff
/// per [`backoff_delay`].
///
/// Returns `Ok(Some(body))` on success, `Ok(None)` on 404, and an error
/// after `max_failures` rate-limit responses or on any transport error.
/// `sleep` is injectable so tests can assert the backoff schedule without
/// waiting; production callers pass `&mut |d| std::thread::sleep(d)`.
pub fn retry_429(
    attempt: &mut dyn FnMut() -> anyhow::Result<FetchOutcome>,
    max_failures: u32,
    sleep: &mut dyn FnMut(Duration),
) -> anyhow::Result<Option<Box<dyn Read + Send>>> {
    let mut failures: u32 = 0;
    loop {
        match attempt()? {
            FetchOutcome::Body(body) => return Ok(Some(body)),
            FetchOutcome::NotFound => return Ok(None),
            FetchOutcome::RateLimited { retry_after_secs } => {
                failures += 1;
                match backoff_delay(retry_after_secs, failures, max_failures) {
                    Some(delay) => sleep(delay),
                    None => {
                        anyhow::bail!("aborting after {failures} rate-limit (429) responses")
                    }
                }
            }
        }
    }
}

/// GET `url` with 429 backoff. See [`retry_429`] for return semantics.
pub fn fetch_with_retry(
    fetcher: &dyn Fetcher,
    url: &str,
    headers: &[(&str, &str)],
    max_failures: u32,
    sleep: &mut dyn FnMut(Duration),
) -> anyhow::Result<Option<Box<dyn Read + Send>>> {
    retry_429(&mut || fetcher.get(url, headers), max_failures, sleep)
        .map_err(|e| e.context(format!("fetching {url}")))
}

/// Read a value from the `meta` table (`None` if the key is absent).
/// One recorded sync attempt. A run that found nothing is still a run:
/// the whole point of the log is telling "checked, nothing new" apart
/// from "never checked".
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub id: i64,
    pub service: String,
    /// "manual" | "auto".
    pub trigger: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub games_imported: i64,
    pub duplicates_skipped: i64,
    pub games_failed: i64,
    pub detail: Option<String>,
    pub error: Option<String>,
}

/// Open a run row the moment work starts, so a sync that crashes or is
/// killed still leaves a trace with no `finished_at`.
pub fn sync_run_start(
    conn: &Connection,
    service: &str,
    trigger: &str,
    started_at: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sync_runs (service, trigger, started_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![service, trigger, started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Close a run with its counts, a human summary, or an error.
#[allow(clippy::too_many_arguments)] // one call site per field; a struct adds noise
pub fn sync_run_finish(
    conn: &Connection,
    id: i64,
    finished_at: &str,
    imported: i64,
    duplicates: i64,
    failed: i64,
    detail: Option<&str>,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_runs SET finished_at = ?2, games_imported = ?3,
             duplicates_skipped = ?4, games_failed = ?5, detail = ?6, error = ?7
         WHERE id = ?1",
        rusqlite::params![id, finished_at, imported, duplicates, failed, detail, error],
    )?;
    Ok(())
}

/// Most recent runs, newest first. `service` None = every service.
pub fn sync_runs(
    conn: &Connection,
    service: Option<&str>,
    limit: u32,
) -> rusqlite::Result<Vec<SyncRun>> {
    let sql = "SELECT id, service, trigger, started_at, finished_at, games_imported,
                      duplicates_skipped, games_failed, detail, error
               FROM sync_runs
               WHERE (?1 IS NULL OR service = ?1)
               ORDER BY id DESC LIMIT ?2";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![service, limit], |r| {
        Ok(SyncRun {
            id: r.get(0)?,
            service: r.get(1)?,
            trigger: r.get(2)?,
            started_at: r.get(3)?,
            finished_at: r.get(4)?,
            games_imported: r.get(5)?,
            duplicates_skipped: r.get(6)?,
            games_failed: r.get(7)?,
            detail: r.get(8)?,
            error: r.get(9)?,
        })
    })?;
    rows.collect()
}

/// When a service last COMPLETED a run without error — the anchor an
/// interval schedule counts from. None = never.
pub fn last_successful_sync(conn: &Connection, service: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT finished_at FROM sync_runs
         WHERE service = ?1 AND error IS NULL AND finished_at IS NOT NULL
         ORDER BY id DESC LIMIT 1",
        [service],
        |r| r.get(0),
    )
    .optional()
}

pub fn meta_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

/// Insert or update a `meta` key.
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod sync_run_tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    #[test]
    fn a_run_that_found_nothing_is_still_recorded() {
        let (_d, conn) = db();
        let id = sync_run_start(&conn, "twic", "auto", "2026-08-02 01:00:00").unwrap();
        sync_run_finish(
            &conn,
            id,
            "2026-08-02 01:00:09",
            0,
            0,
            0,
            Some("up to date — TWIC 1655 is the newest published issue"),
            None,
        )
        .unwrap();

        let runs = sync_runs(&conn, None, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].trigger, "auto");
        assert_eq!(runs[0].games_imported, 0);
        assert!(runs[0].detail.as_deref().unwrap().contains("up to date"));
        assert!(runs[0].error.is_none());
        // The point of the row: this counts as a successful check, so an
        // interval schedule starts counting from here.
        assert_eq!(
            last_successful_sync(&conn, "twic").unwrap().as_deref(),
            Some("2026-08-02 01:00:09")
        );
    }

    #[test]
    fn a_failure_is_history_too_and_does_not_reset_the_schedule() {
        let (_d, conn) = db();
        let ok = sync_run_start(&conn, "chesscom", "manual", "2026-08-01 00:00:00").unwrap();
        sync_run_finish(&conn, ok, "2026-08-01 00:01:00", 12, 3, 0, None, None).unwrap();
        let bad = sync_run_start(&conn, "chesscom", "auto", "2026-08-02 00:00:00").unwrap();
        sync_run_finish(
            &conn,
            bad,
            "2026-08-02 00:00:05",
            0,
            0,
            0,
            None,
            Some("429"),
        )
        .unwrap();

        let runs = sync_runs(&conn, Some("chesscom"), 10).unwrap();
        assert_eq!(runs.len(), 2, "the failure is kept");
        assert_eq!(runs[0].error.as_deref(), Some("429"));
        assert_eq!(
            last_successful_sync(&conn, "chesscom").unwrap().as_deref(),
            Some("2026-08-01 00:01:00"),
            "a failed attempt must not look like a completed one"
        );
    }

    #[test]
    fn an_unfinished_run_leaves_a_trace_and_is_not_a_success() {
        let (_d, conn) = db();
        sync_run_start(&conn, "lichess", "manual", "2026-08-02 10:00:00").unwrap();
        let runs = sync_runs(&conn, Some("lichess"), 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].finished_at.is_none(), "killed mid-run");
        assert!(last_successful_sync(&conn, "lichess").unwrap().is_none());
    }

    #[test]
    fn history_is_newest_first_and_filterable_by_service() {
        let (_d, conn) = db();
        for (svc, at) in [("twic", "1"), ("chesscom", "2"), ("twic", "3")] {
            let id = sync_run_start(&conn, svc, "auto", at).unwrap();
            sync_run_finish(&conn, id, at, 0, 0, 0, None, None).unwrap();
        }
        let all = sync_runs(&conn, None, 10).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].started_at, "3", "newest first");
        let twic = sync_runs(&conn, Some("twic"), 10).unwrap();
        assert_eq!(twic.len(), 2);
        assert!(twic.iter().all(|r| r.service == "twic"));
    }
}
