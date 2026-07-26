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
//! and every request carries the descriptive [`USER_AGENT`].

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
pub const USER_AGENT: &str = "silman/0.1 (personal chess database; contact: contact@kibitzchess.org)";

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
    /// (e.g. `Accept`). Implementations must send [`USER_AGENT`].
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> anyhow::Result<FetchOutcome>;

    /// Perform a form-encoded POST (used by the ficsgames.org client).
    /// Implementations must send [`USER_AGENT`].
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> anyhow::Result<FetchOutcome>;
}

/// Production [`Fetcher`] backed by `ureq` (blocking, serial).
pub struct UreqFetcher;

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
        let mut req = ureq::get(url).set("User-Agent", USER_AGENT);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        outcome(req.call(), url)
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        let req = ureq::post(url).set("User-Agent", USER_AGENT);
        outcome(req.send_form(form), url)
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
