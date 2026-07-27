//! TWIC incremental ingester.
//!
//! The Week in Chess (<https://theweekinchess.com/twic>) publishes a weekly
//! PGN zip per issue at `https://theweekinchess.com/zips/twic{N}g.zip`.
//! [`sync`] resumes from the newest issue recorded in the `twic_issues`
//! table, fetches subsequent issues strictly serially up to a per-run cap,
//! unzips each in memory, imports the games with full provenance, and
//! records the issue. A 404 means no newer issue has been published yet.
//!
//! On the very first run the table is empty and there is nothing to resume
//! from; the caller must supply an explicit starting issue in
//! [`TwicOptions::from`] — we never scrape the site to guess. The report's
//! [`TwicSyncReport::first_run_notice`] then carries [`FIRST_RUN_NOTICE`]
//! for the CLI to print.
//!
//! TWIC's hosting is donation-funded: keep [`TwicOptions::max_issues`]
//! modest, never fetch an issue twice, and never redistribute the data
//! (see CLAUDE.md ground rules).

use std::io::{Cursor, Read};
use std::time::Duration;

use anyhow::Context;
use rusqlite::{params, Connection};

use crate::import::{import_pgn, SourceInfo, SourceKind};
use crate::net::{fetch_with_retry, retry_429, Fetcher, MAX_RATE_LIMIT_FAILURES};

/// Earliest issue whose games zip the TWIC site still serves.
///
/// Empirical basis: `https://theweekinchess.com/zips/twic{N}g.zip` is the
/// downloadable-PGN archive and it starts around issue 920 (June 2012);
/// earlier issues exist only as pages/text on the site, and zip URLs below
/// this number return 404. Rather than re-verifying the whole range (a
/// probe storm on donation-funded bandwidth), callers treat this constant
/// as the floor and verify any single issue with at most one HEAD request
/// at need.
pub const FIRST_AVAILABLE_ISSUE: u32 = 920;

/// Issue↔date anchor for the approximate-week arithmetic: TWIC issue 1000
/// was published on Monday 2014-01-06 (Mark Crowther's TWIC 1000 issue,
/// widely reported at the time). TWIC has been weekly since 1994, so issue
/// N falls approximately (N − 1000) weeks from this anchor. Occasional
/// schedule slips over the years make the arithmetic approximate — UI
/// callers must label derived dates "approx".
pub const ANCHOR_ISSUE: u32 = 1000;

/// 2014-01-06 (the [`ANCHOR_ISSUE`] Monday) as days since the Unix epoch.
/// Cross-checked in tests against the independent civil-date arithmetic in
/// [`crate::net::lichess::utc_timestamp_millis`].
pub const ANCHOR_EPOCH_DAYS: i64 = 16_076;

/// Hard cap on HEAD requests per catalog-refresh probe ([`probe_latest`]).
/// The typical run is 2 requests; this bounds the pathological case.
pub const PROBE_MAX_REQUESTS: u32 = 12;

/// Printed by the CLI when a sync starts from an empty `twic_issues` table.
pub const FIRST_RUN_NOTICE: &str = "\
This looks like your first TWIC sync. The Week in Chess has been compiled \
and published free of charge by Mark Crowther since 1994; its hosting and \
bandwidth are paid for by reader donations. Downloaded issues are for your \
personal use only and must never be redistributed. Learn more at \
https://theweekinchess.com/twic — and if TWIC becomes part of your workflow, \
please consider a donation: the donation and monthly-subscription options \
are on that same page.";

/// License string recorded in the `sources` table for every TWIC import.
pub const TWIC_LICENSE: &str = "TWIC personal use — not redistributable";

/// Options for [`sync`].
#[derive(Debug, Clone)]
pub struct TwicOptions {
    /// Starting issue number for the very first run (when `twic_issues` is
    /// empty). Ignored once at least one issue has been imported — the
    /// ingester then resumes from the newest recorded issue.
    pub from: Option<u32>,
    /// Maximum number of issues to fetch in one run (default 5), so a
    /// long-idle database catches up over several runs instead of hammering
    /// TWIC's donation-funded bandwidth in one go.
    pub max_issues: u32,
}

impl Default for TwicOptions {
    fn default() -> Self {
        Self {
            from: None,
            max_issues: 5,
        }
    }
}

/// Per-issue outcome within a [`TwicSyncReport`].
#[derive(Debug, Clone)]
pub struct TwicIssueReport {
    pub issue: u32,
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    pub games_failed: u64,
}

/// Result of one [`sync`] run.
#[derive(Debug, Clone)]
pub struct TwicSyncReport {
    /// Issues fetched and imported this run, in ascending order.
    pub issues: Vec<TwicIssueReport>,
    /// `true` if the run stopped because the next issue returned 404
    /// (i.e. the database now holds every published issue); `false` if it
    /// stopped at [`TwicOptions::max_issues`] and more may be available.
    pub up_to_date: bool,
    /// Set (to [`FIRST_RUN_NOTICE`]) when this run started from an empty
    /// `twic_issues` table; the CLI must print it.
    pub first_run_notice: Option<String>,
}

/// Download URL for a TWIC issue's PGN zip.
pub fn zip_url(issue: u32) -> String {
    format!("https://theweekinchess.com/zips/twic{issue}g.zip")
}

/// The newest issue recorded in `twic_issues`, if any.
pub fn latest_imported(conn: &Connection) -> rusqlite::Result<Option<u32>> {
    conn.query_row("SELECT MAX(issue) FROM twic_issues", [], |r| {
        r.get::<_, Option<i64>>(0)
    })
    .map(|opt| opt.map(|v| v as u32))
}

/// Every imported issue with its recorded game count, ascending.
pub fn imported_issues(conn: &Connection) -> rusqlite::Result<Vec<(u32, i64)>> {
    let mut stmt = conn.prepare("SELECT issue, games FROM twic_issues ORDER BY issue")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as u32, r.get(1)?)))?;
    rows.collect()
}

/// Civil date from days since the Unix epoch (proleptic Gregorian).
/// Howard Hinnant's `civil_from_days` algorithm — the inverse of the
/// `days_from_civil` used by the Lichess client.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Approximate publication Monday of `issue` as `"YYYY-MM-DD"`, from the
/// weekly arithmetic anchored at [`ANCHOR_ISSUE`]/[`ANCHOR_EPOCH_DAYS`].
/// Approximate by nature — display it labelled "approx".
pub fn approx_date(issue: u32) -> String {
    let days = ANCHOR_EPOCH_DAYS + 7 * (i64::from(issue) - i64::from(ANCHOR_ISSUE));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Approximate current issue number for a day (days since the Unix epoch):
/// the inverse of [`approx_date`], used as [`probe_latest`]'s start guess.
/// Never below [`FIRST_AVAILABLE_ISSUE`].
pub fn estimated_issue(epoch_days: i64) -> u32 {
    let weeks = (epoch_days - ANCHOR_EPOCH_DAYS).div_euclid(7);
    (i64::from(ANCHOR_ISSUE) + weeks).clamp(i64::from(FIRST_AVAILABLE_ISSUE), i64::from(u32::MAX))
        as u32
}

/// Result of one [`probe_latest`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    /// Newest issue known to exist after the probe: an issue confirmed by
    /// a HEAD hit, or the caller's `floor` when nothing above it answered.
    pub latest: Option<u32>,
    /// HEAD requests actually issued (reported to the UI for honesty).
    pub requests: u32,
}

/// Find the newest published issue with a handful of HEAD requests — run
/// ONLY on an explicit user "Refresh catalog" action, never automatically.
///
/// Strategy: start from `guess` (from [`estimated_issue`]; clamped to at
/// least `floor`, the newest issue already known to exist). HEAD the zip:
/// on a hit walk forward until the first 404, on a miss walk backward
/// until the first hit (never below `floor` / [`FIRST_AVAILABLE_ISSUE`]).
/// The weekly arithmetic makes the guess accurate to about a week, so the
/// typical run is 2 requests (guess hits, guess+1 404s); the hard cap is
/// [`PROBE_MAX_REQUESTS`]. 429s are honored via [`retry_429`]
/// (`Retry-After`, else 60 s; abort after [`MAX_RATE_LIMIT_FAILURES`]).
pub fn probe_latest(
    fetcher: &dyn Fetcher,
    floor: Option<u32>,
    guess: u32,
    sleep: &mut dyn FnMut(Duration),
) -> anyhow::Result<ProbeResult> {
    let low = floor.map_or(FIRST_AVAILABLE_ISSUE, |f| f.max(FIRST_AVAILABLE_ISSUE));
    let mut requests: u32 = 0;
    let exists = |issue: u32, sleep: &mut dyn FnMut(Duration)| -> anyhow::Result<bool> {
        let url = zip_url(issue);
        let body = retry_429(&mut || fetcher.head(&url), MAX_RATE_LIMIT_FAILURES, sleep)
            .map_err(|e| e.context(format!("probing {url}")))?;
        Ok(body.is_some())
    };

    let start = guess.max(low);
    requests += 1;
    if exists(start, sleep)? {
        // Walk forward to the first unpublished issue.
        let mut latest = start;
        while requests < PROBE_MAX_REQUESTS {
            requests += 1;
            if exists(latest + 1, sleep)? {
                latest += 1;
            } else {
                return Ok(ProbeResult {
                    latest: Some(latest),
                    requests,
                });
            }
        }
        // Cap reached while still finding issues: report the newest
        // confirmed one (honest lower bound).
        Ok(ProbeResult {
            latest: Some(latest),
            requests,
        })
    } else {
        // Walk backward to the newest published issue. The floor itself
        // is probed only when nothing is known to exist yet (floor=None).
        let lowest_to_probe = if floor.is_some() { low + 1 } else { low };
        let mut i = start;
        while i > lowest_to_probe && requests < PROBE_MAX_REQUESTS {
            i -= 1;
            requests += 1;
            if exists(i, sleep)? {
                return Ok(ProbeResult {
                    latest: Some(i),
                    requests,
                });
            }
        }
        // Nothing above the floor answered; the floor itself (an imported
        // or previously confirmed issue) is still the best-known latest.
        Ok(ProbeResult {
            latest: floor,
            requests,
        })
    }
}

/// Fetch and import one specific TWIC issue with full provenance,
/// recording it in `twic_issues`. Returns `Ok(None)` when the site serves
/// no zip for it (404: unpublished, or outside the zip archive's range).
///
/// Errors if the issue is already recorded — callers must filter against
/// `twic_issues` first, so an issue is never fetched twice (TWIC's
/// bandwidth is donation-funded; see the module docs).
pub fn import_issue(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    issue: u32,
) -> anyhow::Result<Option<TwicIssueReport>> {
    let already: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM twic_issues WHERE issue = ?1)",
            [i64::from(issue)],
            |r| r.get(0),
        )
        .with_context(|| format!("checking twic_issues for {issue}"))?;
    anyhow::ensure!(
        !already,
        "TWIC {issue} is already imported — an issue is never fetched twice"
    );

    let url = zip_url(issue);
    let body = fetch_with_retry(fetcher, &url, &[], MAX_RATE_LIMIT_FAILURES, &mut |d| {
        std::thread::sleep(d)
    })?;
    let Some(mut body) = body else {
        return Ok(None); // 404: not published / not served as a zip.
    };
    let mut zip_bytes = Vec::new();
    body.read_to_end(&mut zip_bytes)
        .with_context(|| format!("downloading {url}"))?;
    let pgn = extract_pgn(&zip_bytes).with_context(|| format!("unzipping {url}"))?;

    let source = SourceInfo {
        name: format!("TWIC {issue}"),
        origin: url.clone(),
        license: TWIC_LICENSE.to_string(),
        kind: SourceKind::Twic,
    };
    let stats = import_pgn(conn, &source, Cursor::new(pgn))
        .with_context(|| format!("importing TWIC {issue}"))?;
    let source_id: i64 = conn.query_row(
        "SELECT id FROM sources WHERE name = ?1 ORDER BY id DESC LIMIT 1",
        [&source.name],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO twic_issues (issue, source_id, games) VALUES (?1, ?2, ?3)",
        params![issue as i64, source_id, stats.games_imported as i64],
    )?;
    Ok(Some(TwicIssueReport {
        issue,
        games_imported: stats.games_imported,
        duplicates_skipped: stats.duplicates_skipped,
        games_failed: stats.games_failed,
    }))
}

/// Incrementally sync TWIC issues into the database.
///
/// Resumes after the newest issue in `twic_issues` (or starts at
/// [`TwicOptions::from`] on the first run — required then, never guessed).
/// Fetches serially until `max_issues` issues have been imported or a 404
/// indicates no newer issue exists. Each issue is imported with a
/// `SourceInfo` naming the issue, its exact URL, and [`TWIC_LICENSE`], and
/// its row is recorded in `twic_issues` immediately after the import so an
/// interrupted run resumes cleanly (re-imported games would be caught by
/// duplicate detection anyway).
pub fn sync(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    opts: &TwicOptions,
) -> anyhow::Result<TwicSyncReport> {
    let (start, first_run) = match latest_imported(conn)? {
        Some(latest) => (latest + 1, false),
        None => match opts.from {
            Some(from) => (from, true),
            None => anyhow::bail!(
                "no TWIC issues imported yet: pass an explicit starting issue \
                 (--from) on the first run"
            ),
        },
    };

    let mut report = TwicSyncReport {
        issues: Vec::new(),
        up_to_date: false,
        first_run_notice: first_run.then(|| FIRST_RUN_NOTICE.to_string()),
    };

    for issue in start..start.saturating_add(opts.max_issues) {
        match import_issue(conn, fetcher, issue)? {
            Some(issue_report) => report.issues.push(issue_report),
            None => {
                // 404: this issue has not been published yet.
                report.up_to_date = true;
                break;
            }
        }
    }
    Ok(report)
}

/// Concatenate every `*.pgn` entry of a zip archive held in memory.
/// TWIC zips normally contain exactly one PGN file.
fn extract_pgn(zip_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.name().to_ascii_lowercase().ends_with(".pgn") {
            entry
                .read_to_end(&mut out)
                .with_context(|| format!("reading zip entry {}", entry.name()))?;
            out.push(b'\n');
        }
    }
    anyhow::ensure!(!out.is_empty(), "zip archive contains no .pgn entry");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented anchor day must agree with the independently tested
    /// civil-date arithmetic in the Lichess client.
    #[test]
    fn anchor_epoch_days_matches_lichess_arithmetic() {
        assert_eq!(
            crate::net::lichess::utc_timestamp_millis("2014.01.06", "00:00:00"),
            Some(ANCHOR_EPOCH_DAYS * 86_400_000)
        );
    }

    #[test]
    fn approx_date_at_and_around_the_anchor() {
        assert_eq!(approx_date(ANCHOR_ISSUE), "2014-01-06");
        assert_eq!(approx_date(ANCHOR_ISSUE + 1), "2014-01-13");
        assert_eq!(approx_date(ANCHOR_ISSUE - 1), "2013-12-30");
        // The archive floor, 80 weeks before the anchor: Monday 2012-06-25.
        assert_eq!(approx_date(FIRST_AVAILABLE_ISSUE), "2012-06-25");
    }

    #[test]
    fn estimated_issue_inverts_approx_date() {
        assert_eq!(estimated_issue(ANCHOR_EPOCH_DAYS), ANCHOR_ISSUE);
        // Any day within an issue's week maps back to that issue.
        assert_eq!(estimated_issue(ANCHOR_EPOCH_DAYS + 6), ANCHOR_ISSUE);
        assert_eq!(estimated_issue(ANCHOR_EPOCH_DAYS + 7), ANCHOR_ISSUE + 1);
        // Days before the archive floor clamp to the floor.
        assert_eq!(estimated_issue(0), FIRST_AVAILABLE_ISSUE);
        // Round-trip property across a few decades of issues.
        for issue in [920u32, 1000, 1234, 1580, 1700, 2000] {
            let days = ANCHOR_EPOCH_DAYS + 7 * (i64::from(issue) - i64::from(ANCHOR_ISSUE));
            assert_eq!(estimated_issue(days), issue, "issue {issue}");
        }
    }
}
