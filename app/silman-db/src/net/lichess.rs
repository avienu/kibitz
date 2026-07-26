//! Lichess games-export client.
//!
//! Streams a user's full game export
//! (`GET https://lichess.org/api/games/user/{username}`, requested as
//! `application/x-chess-pgn`) to a temporary file, imports it with
//! provenance, and remembers the newest game's UTC timestamp under the meta
//! key `lichess_since_{username}` so the next [`sync_user`] run passes
//! `since=` and downloads only newer games.
//!
//! Rate limiting follows Lichess guidance: strictly serial requests, a
//! descriptive User-Agent ([`crate::net::USER_AGENT`]), and on 429 wait
//! `Retry-After` (else 60 s) before retrying, aborting after
//! [`crate::net::MAX_RATE_LIMIT_FAILURES`] failures.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::Context;
use rusqlite::Connection;

use crate::import::{import_pgn, SourceInfo, SourceKind};
use crate::net::{fetch_with_retry, meta_get, meta_set, Fetcher, MAX_RATE_LIMIT_FAILURES};

/// License string recorded in `sources` for Lichess imports.
pub const LICHESS_LICENSE: &str =
    "Lichess games export — personal archive (lichess.org API terms of use)";

/// Result of one [`sync_user`] run.
#[derive(Debug, Clone)]
pub struct LichessSyncReport {
    pub username: String,
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    pub games_failed: u64,
    /// New value stored under the `lichess_since_{username}` meta key
    /// (newest game's UTC timestamp in ms + 1), if any game carried
    /// `UTCDate`/`UTCTime` tags. `None` leaves the previous value in place.
    pub new_since_millis: Option<i64>,
}

/// Meta-table key holding the resume timestamp (ms since epoch) for a user.
pub fn meta_key(username: &str) -> String {
    format!("lichess_since_{}", username.to_ascii_lowercase())
}

/// Export URL for a user, with the optional incremental `since` cursor.
pub fn export_url(username: &str, since_millis: Option<i64>) -> String {
    let mut url = format!(
        "https://lichess.org/api/games/user/{username}\
         ?pgnInJson=false&clocks=false&evals=false&opening=false"
    );
    if let Some(since) = since_millis {
        url.push_str(&format!("&since={since}"));
    }
    url
}

/// Download and import a user's Lichess games, resuming incrementally.
///
/// The response is streamed to a temporary file (removed afterwards), the
/// newest `UTCDate`/`UTCTime` pair is extracted, the file is imported via
/// [`import_pgn`] with a `SourceInfo` recording the exact request URL and
/// [`LICHESS_LICENSE`], and on success the resume cursor is advanced.
pub fn sync_user(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    username: &str,
) -> anyhow::Result<LichessSyncReport> {
    let key = meta_key(username);
    let since = meta_get(conn, &key)?.and_then(|v| v.parse::<i64>().ok());
    let url = export_url(username, since);

    let body = fetch_with_retry(
        fetcher,
        &url,
        &[("Accept", "application/x-chess-pgn")],
        MAX_RATE_LIMIT_FAILURES,
        &mut |d| std::thread::sleep(d),
    )?;
    let Some(mut body) = body else {
        anyhow::bail!("lichess returned 404 for user {username:?} (unknown user?)");
    };

    // Stream to a temp file so huge exports never sit in memory.
    let tmp = TempFile::new(&format!("silman-lichess-{username}"));
    {
        let mut out =
            File::create(&tmp.path).with_context(|| format!("creating {}", tmp.path.display()))?;
        std::io::copy(&mut body, &mut out).context("streaming lichess export")?;
        out.flush()?;
    }

    let newest = newest_game_millis(BufReader::new(File::open(&tmp.path)?))?;
    let source = SourceInfo {
        name: format!("Lichess: {username}"),
        origin: url,
        license: LICHESS_LICENSE.to_string(),
        kind: SourceKind::Online,
    };
    let stats = import_pgn(conn, &source, BufReader::new(File::open(&tmp.path)?))
        .with_context(|| format!("importing lichess games for {username}"))?;

    let new_since = newest.map(|ms| ms + 1);
    if let Some(since) = new_since {
        meta_set(conn, &key, &since.to_string())?;
    }
    Ok(LichessSyncReport {
        username: username.to_string(),
        games_imported: stats.games_imported,
        duplicates_skipped: stats.duplicates_skipped,
        games_failed: stats.games_failed,
        new_since_millis: new_since,
    })
}

/// Newest `UTCDate`+`UTCTime` pair in a PGN stream, as ms since the Unix
/// epoch. Cheap tag-line scan; movetext lines are ignored.
fn newest_game_millis<R: BufRead>(reader: R) -> anyhow::Result<Option<i64>> {
    let mut newest: Option<i64> = None;
    let mut date: Option<String> = None;
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if let Some(v) = tag_value(trimmed, "UTCDate") {
            date = Some(v.to_string());
        } else if let Some(v) = tag_value(trimmed, "UTCTime") {
            if let Some(ms) = date.as_deref().and_then(|d| utc_timestamp_millis(d, v)) {
                newest = Some(newest.map_or(ms, |n| n.max(ms)));
            }
        }
    }
    Ok(newest)
}

/// Extract the value of `[Key "Value"]` if `line` is that tag pair.
fn tag_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix('[')?
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('"')?
        .strip_suffix("\"]")
}

/// Convert PGN `UTCDate` (`YYYY.MM.DD`) + `UTCTime` (`HH:MM:SS`) to ms
/// since the Unix epoch. Returns `None` for unknown (`????.??.??`) or
/// malformed values. Pure and clock-free, so it is unit-testable.
pub fn utc_timestamp_millis(date: &str, time: &str) -> Option<i64> {
    let mut d = date.split('.');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.split(':');
    let hour: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let sec: i64 = t.next()?.parse().ok()?;
    if !(0..24).contains(&hour) || !(0..60).contains(&min) || !(0..61).contains(&sec) {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + min * 60 + sec;
    Some(secs * 1_000)
}

/// Days from 1970-01-01 to the given civil date (proleptic Gregorian).
/// Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Temp file removed on drop. Lives in the OS temp dir; the name is unique
/// per process and call.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(prefix: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{n}.pgn", std::process::id()));
        Self { path }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
