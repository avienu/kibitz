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

use anyhow::Context;
use rusqlite::{params, Connection};

use crate::import::{import_pgn, SourceInfo};
use crate::net::{fetch_with_retry, Fetcher, MAX_RATE_LIMIT_FAILURES};

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
        let url = zip_url(issue);
        let body = fetch_with_retry(fetcher, &url, &[], MAX_RATE_LIMIT_FAILURES, &mut |d| {
            std::thread::sleep(d)
        })?;
        let Some(mut body) = body else {
            // 404: this issue has not been published yet.
            report.up_to_date = true;
            break;
        };
        let mut zip_bytes = Vec::new();
        body.read_to_end(&mut zip_bytes)
            .with_context(|| format!("downloading {url}"))?;
        let pgn = extract_pgn(&zip_bytes).with_context(|| format!("unzipping {url}"))?;

        let source = SourceInfo {
            name: format!("TWIC {issue}"),
            origin: url.clone(),
            license: TWIC_LICENSE.to_string(),
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
        report.issues.push(TwicIssueReport {
            issue,
            games_imported: stats.games_imported,
            duplicates_skipped: stats.duplicates_skipped,
            games_failed: stats.games_failed,
        });
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
