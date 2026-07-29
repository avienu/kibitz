//! Tests for the chess.com monthly-archives client. Everything here is
//! offline except the clearly-labeled live test at the bottom (gated on
//! KIBITZ_NET_TESTS=1).

mod common;

use common::{live_tests_enabled, meta, open_temp_db, FixtureFetcher, Scripted};
use kibitz_db::net::chesscom;

const ARCHIVES_JSON: &[u8] = include_bytes!("fixtures/chesscom_archives.json");
const PGN_JAN: &[u8] = include_bytes!("fixtures/chesscom_2024_01.pgn");
const PGN_FEB: &[u8] = include_bytes!("fixtures/chesscom_2024_02.pgn");
const PGN_MAR: &[u8] = include_bytes!("fixtures/chesscom_2024_03.pgn");

#[test]
fn url_construction() {
    assert_eq!(
        chesscom::archives_url("testuser"),
        "https://api.chess.com/pub/player/testuser/games/archives"
    );
    assert_eq!(
        chesscom::month_pgn_url("testuser", "2024/01"),
        "https://api.chess.com/pub/player/testuser/games/2024/01/pgn"
    );
}

#[test]
fn parse_archives_sorts_and_ignores_junk() {
    let json = r#"{"archives": [
        "https://api.chess.com/pub/player/u/games/2024/02",
        "https://api.chess.com/pub/player/u/games/2023/12",
        "https://api.chess.com/pub/player/u/games/not/amonth",
        "https://api.chess.com/pub/player/u/games/2024/01"
    ]}"#;
    assert_eq!(
        chesscom::parse_archives(json).unwrap(),
        vec!["2023/12", "2024/01", "2024/02"]
    );
    assert!(chesscom::parse_archives("{}").is_err());
    assert!(chesscom::parse_archives("not json").is_err());
}

#[test]
fn first_sync_imports_all_months_oldest_first_and_records_cursor() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/01"),
        Scripted::Body(PGN_JAN.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::Body(PGN_FEB.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );

    let report = chesscom::sync_user(&conn, &fetcher, "testuser").unwrap();

    // Serial: archives index first, then months oldest → newest.
    assert_eq!(
        fetcher.requested_urls(),
        vec![
            chesscom::archives_url("testuser"),
            chesscom::month_pgn_url("testuser", "2024/01"),
            chesscom::month_pgn_url("testuser", "2024/02"),
            chesscom::month_pgn_url("testuser", "2024/03"),
        ]
    );
    assert_eq!(report.months.len(), 3);
    assert_eq!(report.months[0].month, "2024/01");
    assert_eq!(report.months[0].games_imported, 1);
    assert_eq!(report.months[2].month, "2024/03");

    // The newest month is never recorded as fully imported.
    assert_eq!(report.last_recorded_month.as_deref(), Some("2024/02"));
    assert_eq!(
        meta(&conn, "chesscom_last_month_testuser").as_deref(),
        Some("2024/02")
    );

    // Provenance for one month.
    let (origin, license): (String, String) = conn
        .query_row(
            "SELECT origin, license FROM sources WHERE name = 'chess.com: testuser 2024/01'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(origin, chesscom::month_pgn_url("testuser", "2024/01"));
    assert_eq!(license, chesscom::CHESSCOM_LICENSE);
}

#[test]
fn second_sync_refetches_only_unrecorded_months_and_skips_dups() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/01"),
        Scripted::Body(PGN_JAN.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::Body(PGN_FEB.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );
    chesscom::sync_user(&conn, &fetcher, "testuser").unwrap();

    // Second run: only the archives index and the still-open newest month.
    let fetcher2 = FixtureFetcher::new();
    fetcher2.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher2.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );
    let report = chesscom::sync_user(&conn, &fetcher2, "testuser").unwrap();

    assert_eq!(
        fetcher2.requested_urls(),
        vec![
            chesscom::archives_url("testuser"),
            chesscom::month_pgn_url("testuser", "2024/03"),
        ]
    );
    assert_eq!(report.months.len(), 1);
    assert_eq!(report.months[0].month, "2024/03");
    assert_eq!(report.months[0].games_imported, 0, "re-fetch is all dups");
    assert_eq!(report.months[0].duplicates_skipped, 1);
}

#[test]
fn unknown_user_404_is_an_error() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&chesscom::archives_url("nosuchuser"), Scripted::NotFound);
    let err = chesscom::sync_user(&conn, &fetcher, "nosuchuser").unwrap_err();
    assert!(err.to_string().contains("404"), "got: {err}");
}

/// LIVE NETWORK TEST — runs only with KIBITZ_NET_TESTS=1. Fetches only the
/// tiny archives index (a short JSON list) for a well-known account.
#[test]
fn live_chesscom_archives_index() {
    if !live_tests_enabled("live_chesscom_archives_index") {
        return;
    }
    use kibitz_db::net::{FetchOutcome, Fetcher, UreqFetcher};
    use std::io::Read;
    let outcome = UreqFetcher
        .get(
            &chesscom::archives_url("erik"),
            &[("Accept", "application/json")],
        )
        .unwrap();
    match outcome {
        FetchOutcome::Body(mut body) => {
            let mut json = String::new();
            body.read_to_string(&mut json).unwrap();
            let months = chesscom::parse_archives(&json).unwrap();
            assert!(!months.is_empty(), "erik has archives");
        }
        FetchOutcome::NotFound => panic!("user erik should exist"),
        FetchOutcome::RateLimited { .. } => eprintln!("rate limited; treating as pass"),
    }
}

/// Observed sync: months report an honest fraction and a stop between
/// months keeps the cursor at the last COMPLETED month (run-9 report:
/// "no clue as to status... will it resume where I left off?").
#[test]
fn observed_sync_reports_months_and_stops_cleanly() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/01"),
        Scripted::Body(PGN_JAN.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::Body(PGN_FEB.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );
    let mut seen: Vec<(usize, usize, String)> = Vec::new();
    // Stop after observing the first month.
    let report = chesscom::sync_user_observed(&conn, &fetcher, "testuser", &mut |d, t, m, _g| {
        seen.push((d, t, m.to_string()));
        seen.len() < 2
    })
    .unwrap();
    assert!(!seen.is_empty(), "observer ran");
    assert!(seen.iter().all(|(d, t, _)| d < t), "honest fractions");
    // Cursor only covers fully-imported months; resuming re-runs from the
    // first month that was not completed (fresh fetcher: scripts are
    // one-shot).
    let fetcher2 = FixtureFetcher::new();
    fetcher2.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher2.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::Body(PGN_FEB.to_vec()),
    );
    fetcher2.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );
    let resumed =
        chesscom::sync_user_observed(&conn, &fetcher2, "testuser", &mut |_, _, _, _| true).unwrap();
    let all_months: usize = report.months.len() + resumed.months.len();
    assert!(all_months >= 2, "resume picked up the remaining months");
}

/// The 2026-07-28 field report: chess.com throttled a burst by stalling
/// mid-response-body; without fetcher timeouts the read blocked forever
/// and the sync wedged at the same month for a day. With timeouts the
/// stall surfaces as a mid-body read error — this pins the recovery
/// contract: the month FAILS the sync (visibly), the cursor stays at the
/// last COMPLETED month, and the next run resumes and finishes.
#[test]
fn mid_body_stall_fails_visibly_and_the_next_run_resumes() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/01"),
        Scripted::Body(PGN_JAN.to_vec()),
    );
    // 2024/02 stalls mid-download (prefix bytes, then a read timeout).
    fetcher.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::BrokenBody(PGN_FEB[..40].to_vec()),
    );
    let err = chesscom::sync_user(&conn, &fetcher, "testuser").unwrap_err();
    assert!(
        format!("{err:#}").contains("2024/02"),
        "the failing month is named: {err:#}"
    );
    // Cursor holds at the last fully-imported month.
    assert_eq!(
        meta(&conn, "chesscom_last_month_testuser").as_deref(),
        Some("2024/01")
    );

    // The next run resumes at 2024/02 and completes.
    let fetcher2 = FixtureFetcher::new();
    fetcher2.script(
        &chesscom::archives_url("testuser"),
        Scripted::Body(ARCHIVES_JSON.to_vec()),
    );
    fetcher2.script(
        &chesscom::month_pgn_url("testuser", "2024/02"),
        Scripted::Body(PGN_FEB.to_vec()),
    );
    fetcher2.script(
        &chesscom::month_pgn_url("testuser", "2024/03"),
        Scripted::Body(PGN_MAR.to_vec()),
    );
    let report = chesscom::sync_user(&conn, &fetcher2, "testuser").unwrap();
    assert_eq!(report.months.len(), 2, "2024/02 and 2024/03");
    assert_eq!(
        meta(&conn, "chesscom_last_month_testuser").as_deref(),
        Some("2024/02"),
        "newest month still never recorded complete"
    );
}
