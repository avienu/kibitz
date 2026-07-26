//! FICS games retrieval via ficsgames.org (the community FICS archive).
//!
//! # Investigation result (July 2026): scriptable, with care
//!
//! FICS itself is telnet-only, but <https://www.ficsgames.org> archives all
//! FICS games and its download form is plain HTML — no captcha, login,
//! session, or email step. Verbatim from
//! `https://www.ficsgames.org/download.html`:
//!
//! ```html
//! <form name="downloadform" action="/cgi-bin/download.cgi" method="post">
//! ```
//!
//! with fields `gametype` (radio; `11` = "Games of player", paired with the
//! text input `player`, max length 17), `year` (1999..current), `month`
//! (`0` = whole year, `1`–`12`), `movetimes` (`0`/`1`) and the submit button
//! `download=Download`. One POST per player+year (or +month) returns the
//! games directly as a compressed archive (the site mentions zip and bzip2;
//! player downloads are typically `.pgn.bz2`).
//!
//! # Terms-of-use concerns (read before enabling this in a UI)
//!
//! - `https://www.ficsgames.org/robots.txt` contains `Disallow: /cgi-bin`
//!   for all user agents. That is crawler guidance, and this client is a
//!   user-initiated, single-request tool rather than a crawler — but it
//!   signals the operator does not want automated traffic on that CGI.
//!   Be conservative: strictly serial requests, one year/month per call, a
//!   descriptive User-Agent, and no bulk scraping.
//! - `download.html` notes bandwidth is limited and the big monthly
//!   archives are quota-limited. Site contact: `fics.ludens@gmail.com`.
//! - No formal terms-of-use or license page exists on the site.
//!
//! # Residual uncertainty
//!
//! The form's method/action/fields above were read from the live page HTML,
//! but the response format was not verified end-to-end: a single test POST
//! (player + one month) received no response within 90 s — the CGI appears
//! to generate archives slowly ("Processing download request, please
//! wait..." on the site). Callers should therefore use a generous request
//! timeout, and [`sync_user`] deliberately dispatches on the response's
//! magic bytes rather than assuming one format.
//!
//! # Behavior
//!
//! [`sync_user`] issues one POST for one player and one year (optionally a
//! single month), then dispatches on the response's magic bytes: zip is
//! unpacked in memory and imported; plain PGN is imported directly; a bzip2
//! archive cannot be decompressed in-process (no bzip2 dependency in this
//! workspace), so it is saved to a file and the report carries the path plus
//! instructions ([`FicsSyncReport::saved_archive`]) — decompress with
//! `bunzip2` and run `import-pgn`. Anything else (an HTML error page for an
//! unknown player, quota message, ...) is an error carrying a snippet of
//! the response.

use std::io::{Cursor, Read};
use std::path::PathBuf;

use anyhow::Context;
use rusqlite::Connection;

use crate::import::{import_pgn, SourceInfo};
use crate::net::{retry_429, Fetcher, MAX_RATE_LIMIT_FAILURES};

/// The ficsgames.org download CGI (POST, form-encoded).
pub const DOWNLOAD_CGI: &str = "https://www.ficsgames.org/cgi-bin/download.cgi";

/// License string recorded in `sources` for ficsgames.org imports.
pub const FICS_LICENSE: &str =
    "FICS games via ficsgames.org — personal use (site contact: fics.ludens@gmail.com)";

/// Result of one [`sync_user`] run.
#[derive(Debug, Clone)]
pub struct FicsSyncReport {
    pub username: String,
    pub year: u16,
    /// `None` = the whole year was requested.
    pub month: Option<u8>,
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    pub games_failed: u64,
    /// Set when the server returned a bzip2 archive we cannot decompress
    /// in-process: the raw `.pgn.bz2` was saved here. The CLI should tell
    /// the user to run `bunzip2` on it and import the resulting PGN.
    pub saved_archive: Option<PathBuf>,
}

/// Form parameters for a "Games of player" download, exactly as the
/// ficsgames.org form submits them.
pub fn form_params(
    username: &str,
    year: u16,
    month: Option<u8>,
    movetimes: bool,
) -> Vec<(String, String)> {
    vec![
        ("gametype".into(), "11".into()),
        ("player".into(), username.into()),
        ("year".into(), year.to_string()),
        ("month".into(), month.unwrap_or(0).to_string()),
        ("movetimes".into(), if movetimes { "1" } else { "0" }.into()),
        ("download".into(), "Download".into()),
    ]
}

/// What the download CGI handed back, judged by magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    Zip,
    Bzip2,
    Pgn,
    Unknown,
}

/// Classify a ficsgames.org response body by its magic bytes.
pub fn classify(bytes: &[u8]) -> ResponseKind {
    if bytes.starts_with(b"PK\x03\x04") {
        ResponseKind::Zip
    } else if bytes.starts_with(b"BZh") {
        ResponseKind::Bzip2
    } else {
        // PGN starts with a tag section or an escape/comment line.
        let head = bytes.iter().position(|b| !b.is_ascii_whitespace());
        match head.map(|i| bytes[i]) {
            Some(b'[') | Some(b'%') | Some(b';') => ResponseKind::Pgn,
            _ => ResponseKind::Unknown,
        }
    }
}

/// Download and import one player's FICS games for one year (or one month).
///
/// Issues a single serial POST to [`DOWNLOAD_CGI`] (429-backoff via
/// [`retry_429`]) and imports the result with provenance ([`FICS_LICENSE`],
/// origin = CGI URL plus the query fields). There is no incremental cursor:
/// the caller chooses the year/month explicitly, and duplicate detection
/// makes re-runs harmless. See the module docs for the bzip2 fallback.
pub fn sync_user(
    conn: &Connection,
    fetcher: &dyn Fetcher,
    username: &str,
    year: u16,
    month: Option<u8>,
) -> anyhow::Result<FicsSyncReport> {
    if let Some(m) = month {
        anyhow::ensure!((1..=12).contains(&m), "month must be 1-12, got {m}");
    }
    let params = form_params(username, year, month, false);
    let form: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let body = retry_429(
        &mut || fetcher.post_form(DOWNLOAD_CGI, &form),
        MAX_RATE_LIMIT_FAILURES,
        &mut |d| std::thread::sleep(d),
    )?;
    let Some(mut body) = body else {
        anyhow::bail!("ficsgames.org returned 404 for the download CGI");
    };
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .context("downloading ficsgames.org archive")?;

    let month_str = month.map_or_else(|| "whole year".to_string(), |m| format!("month {m:02}"));
    let mut report = FicsSyncReport {
        username: username.to_string(),
        year,
        month,
        games_imported: 0,
        duplicates_skipped: 0,
        games_failed: 0,
        saved_archive: None,
    };
    let source = SourceInfo {
        name: format!("FICS: {username} {year} ({month_str})"),
        origin: format!(
            "{DOWNLOAD_CGI} (POST gametype=11&player={username}&year={year}&month={})",
            month.unwrap_or(0)
        ),
        license: FICS_LICENSE.to_string(),
    };

    match classify(&bytes) {
        ResponseKind::Zip => {
            let pgn = extract_zip_pgn(&bytes).context("unzipping ficsgames.org archive")?;
            let stats = import_pgn(conn, &source, Cursor::new(pgn))?;
            report.games_imported = stats.games_imported;
            report.duplicates_skipped = stats.duplicates_skipped;
            report.games_failed = stats.games_failed;
        }
        ResponseKind::Pgn => {
            let stats = import_pgn(conn, &source, Cursor::new(bytes))?;
            report.games_imported = stats.games_imported;
            report.duplicates_skipped = stats.duplicates_skipped;
            report.games_failed = stats.games_failed;
        }
        ResponseKind::Bzip2 => {
            let path = std::env::temp_dir().join(format!(
                "fics_{username}_{year}_{}.pgn.bz2",
                month.unwrap_or(0)
            ));
            std::fs::write(&path, &bytes)
                .with_context(|| format!("saving archive to {}", path.display()))?;
            report.saved_archive = Some(path);
        }
        ResponseKind::Unknown => {
            let snippet: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
            anyhow::bail!(
                "ficsgames.org returned neither an archive nor PGN (unknown player, \
                 quota, or site change?); response starts: {snippet:?}"
            );
        }
    }
    Ok(report)
}

/// Concatenate every `*.pgn` entry of an in-memory zip archive.
fn extract_zip_pgn(zip_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.name().to_ascii_lowercase().ends_with(".pgn") {
            entry.read_to_end(&mut out)?;
            out.push(b'\n');
        }
    }
    anyhow::ensure!(!out.is_empty(), "zip archive contains no .pgn entry");
    Ok(out)
}
