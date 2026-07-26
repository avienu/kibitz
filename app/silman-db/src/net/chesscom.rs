//! chess.com monthly-archives client.
//!
//! The chess.com published-data API lists a player's completed months at
//! `GET https://api.chess.com/pub/player/{username}/games/archives` (JSON:
//! `{"archives": [month URLs]}`) and serves each month's games as PGN at
//! `{archive}/pgn`. [`sync_user`] walks the months strictly serially,
//! oldest first, skipping months at or before the last fully-imported month
//! recorded under the meta key `chesscom_last_month_{username}`.
//!
//! The final (newest) month in the archives list may still be receiving
//! games, so it is imported but never recorded as fully imported; the next
//! run re-fetches it and duplicate detection skips the games already seen.
//!
//! Rate limiting: serial requests only (chess.com allows unlimited serial
//! access but returns 429 on concurrency/abuse), descriptive User-Agent,
//! and 429 backoff via [`crate::net::fetch_with_retry`].

use std::io::{BufReader, Read};

use anyhow::Context;
use rusqlite::Connection;

use crate::import::{import_pgn, SourceInfo};
use crate::net::{fetch_with_retry, meta_get, meta_set, Fetcher, MAX_RATE_LIMIT_FAILURES};

/// License string recorded in `sources` for chess.com imports.
pub const CHESSCOM_LICENSE: &str =
    "chess.com published-data API — personal archive (chess.com terms of service)";

/// Result of importing one month.
#[derive(Debug, Clone)]
pub struct ChesscomMonthReport {
    /// Month in `yyyy/mm` form.
    pub month: String,
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    pub games_failed: u64,
}

/// Result of one [`sync_user`] run.
#[derive(Debug, Clone)]
pub struct ChesscomSyncReport {
    pub username: String,
    /// Months fetched this run, oldest first.
    pub months: Vec<ChesscomMonthReport>,
    /// The meta cursor after this run: the newest month recorded as fully
    /// imported (the final archive month is intentionally never recorded).
    pub last_recorded_month: Option<String>,
}

/// Meta-table key holding the last fully-imported month (`yyyy/mm`).
pub fn meta_key(username: &str) -> String {
    format!("chesscom_last_month_{}", username.to_ascii_lowercase())
}

/// URL of the player's monthly-archives index.
pub fn archives_url(username: &str) -> String {
    format!("https://api.chess.com/pub/player/{username}/games/archives")
}

/// URL of one month's games as PGN.
pub fn month_pgn_url(username: &str, month: &str) -> String {
    format!("https://api.chess.com/pub/player/{username}/games/{month}/pgn")
}

/// Parse the archives JSON into sorted, deduplicated `yyyy/mm` strings.
/// Entries whose URLs do not end in `/{yyyy}/{mm}` are ignored.
pub fn parse_archives(json: &str) -> anyhow::Result<Vec<String>> {
    let value: serde_json::Value =
        serde_json::from_str(json).context("parsing chess.com archives JSON")?;
    let list = value
        .get("archives")
        .and_then(|a| a.as_array())
        .ok_or_else(|| anyhow::anyhow!("archives JSON has no \"archives\" array"))?;
    let mut months: Vec<String> = list
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(month_from_url)
        .collect();
    months.sort();
    months.dedup();
    Ok(months)
}

/// Extract `yyyy/mm` from an archive URL ending in `/{yyyy}/{mm}`.
fn month_from_url(url: &str) -> Option<String> {
    let mut parts = url.trim_end_matches('/').rsplit('/');
    let mm = parts.next()?;
    let yyyy = parts.next()?;
    let numeric = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    (yyyy.len() == 4 && mm.len() == 2 && numeric(yyyy) && numeric(mm))
        .then(|| format!("{yyyy}/{mm}"))
}

/// Download and import a user's chess.com games, resuming incrementally.
///
/// Fetches the archives index, then each unimported month serially, oldest
/// first, importing via [`import_pgn`] with a `SourceInfo` recording the
/// month URL and [`CHESSCOM_LICENSE`]. After each successfully imported
/// month except the final (possibly still growing) one, the meta cursor is
/// advanced, so an interrupted run resumes at the right month.
pub fn sync_user(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    username: &str,
) -> anyhow::Result<ChesscomSyncReport> {
    let index_url = archives_url(username);
    let body = fetch_with_retry(
        fetcher,
        &index_url,
        &[("Accept", "application/json")],
        MAX_RATE_LIMIT_FAILURES,
        &mut |d| std::thread::sleep(d),
    )?;
    let Some(mut body) = body else {
        anyhow::bail!("chess.com returned 404 for user {username:?} (unknown user?)");
    };
    let mut json = String::new();
    body.read_to_string(&mut json)
        .context("downloading chess.com archives index")?;
    let months = parse_archives(&json)?;

    let key = meta_key(username);
    let last = meta_get(conn, &key)?;
    let mut report = ChesscomSyncReport {
        username: username.to_string(),
        months: Vec::new(),
        last_recorded_month: last.clone(),
    };

    let total = months.len();
    for (i, month) in months.iter().enumerate() {
        if last.as_deref().is_some_and(|l| month.as_str() <= l) {
            continue; // already fully imported
        }
        let url = month_pgn_url(username, month);
        let body = fetch_with_retry(
            fetcher,
            &url,
            &[("Accept", "application/x-chess-pgn")],
            MAX_RATE_LIMIT_FAILURES,
            &mut |d| std::thread::sleep(d),
        )?;
        let Some(body) = body else {
            // Month listed but gone: skip rather than fail the whole sync.
            continue;
        };
        let source = SourceInfo {
            name: format!("chess.com: {username} {month}"),
            origin: url,
            license: CHESSCOM_LICENSE.to_string(),
        };
        let stats = import_pgn(conn, &source, BufReader::new(body))
            .with_context(|| format!("importing chess.com {username} {month}"))?;
        report.months.push(ChesscomMonthReport {
            month: month.clone(),
            games_imported: stats.games_imported,
            duplicates_skipped: stats.duplicates_skipped,
            games_failed: stats.games_failed,
        });
        // The newest month may still gain games; never mark it complete.
        if i + 1 < total {
            meta_set(conn, &key, month)?;
            report.last_recorded_month = Some(month.clone());
        }
    }
    Ok(report)
}
