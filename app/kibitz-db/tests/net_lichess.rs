//! Tests for the Lichess export client. Everything here is offline except
//! the clearly-labeled live test at the bottom (gated on KIBITZ_NET_TESTS=1).

mod common;

use common::{live_tests_enabled, meta, open_temp_db, FixtureFetcher, Scripted};
use kibitz_db::net::lichess;

const FIXTURE_PGN: &[u8] = include_bytes!("fixtures/lichess_testuser.pgn");

/// 2024-02-20T08:00:00Z (the newest game in the fixture), in Unix ms.
const NEWEST_MS: i64 = 1_708_416_000_000;

#[test]
fn export_url_construction() {
    assert_eq!(
        lichess::export_url("TestUser", None),
        "https://lichess.org/api/games/user/TestUser\
         ?pgnInJson=false&clocks=false&evals=false&opening=false"
    );
    assert!(lichess::export_url("u", Some(123)).ends_with("&since=123"));
}

#[test]
fn utc_timestamp_millis_known_values() {
    assert_eq!(
        lichess::utc_timestamp_millis("1970.01.01", "00:00:00"),
        Some(0)
    );
    assert_eq!(
        lichess::utc_timestamp_millis("2024.02.20", "08:00:00"),
        Some(NEWEST_MS)
    );
    // Unknown / malformed values must not produce a cursor.
    assert_eq!(
        lichess::utc_timestamp_millis("????.??.??", "00:00:00"),
        None
    );
    assert_eq!(
        lichess::utc_timestamp_millis("2024.13.01", "00:00:00"),
        None
    );
    assert_eq!(
        lichess::utc_timestamp_millis("2024.01.01", "25:00:00"),
        None
    );
}

#[test]
fn first_sync_imports_and_records_since_cursor() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    let url = lichess::export_url("testuser", None);
    fetcher.script(&url, Scripted::Body(FIXTURE_PGN.to_vec()));

    let report = lichess::sync_user(&conn, &fetcher, "testuser").unwrap();

    assert_eq!(report.games_imported, 2);
    assert_eq!(report.games_failed, 0);
    assert_eq!(report.new_since_millis, Some(NEWEST_MS + 1));
    assert_eq!(
        meta(&conn, "lichess_since_testuser").as_deref(),
        Some((NEWEST_MS + 1).to_string().as_str()),
        "cursor = newest game's UTC timestamp + 1"
    );

    // Exactly one request, with the PGN Accept header.
    assert_eq!(fetcher.requested_urls(), vec![url]);
    assert!(fetcher
        .headers_of(0)
        .contains(&("Accept".to_string(), "application/x-chess-pgn".to_string())));

    // Provenance recorded.
    let (origin, license): (String, String) = conn
        .query_row(
            "SELECT origin, license FROM sources WHERE name = 'Lichess: testuser'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(origin.starts_with("https://lichess.org/api/games/user/testuser"));
    assert_eq!(license, lichess::LICHESS_LICENSE);
}

#[test]
fn second_sync_resumes_from_cursor_and_keeps_it_on_empty_export() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &lichess::export_url("testuser", None),
        Scripted::Body(FIXTURE_PGN.to_vec()),
    );
    lichess::sync_user(&conn, &fetcher, "testuser").unwrap();

    // Second run must request with since=<cursor>; empty body = no new games.
    let resumed_url = lichess::export_url("testuser", Some(NEWEST_MS + 1));
    let fetcher2 = FixtureFetcher::new();
    fetcher2.script(&resumed_url, Scripted::Body(Vec::new()));
    let report = lichess::sync_user(&conn, &fetcher2, "testuser").unwrap();

    assert_eq!(fetcher2.requested_urls(), vec![resumed_url]);
    assert_eq!(report.games_imported, 0);
    assert_eq!(report.new_since_millis, None);
    assert_eq!(
        meta(&conn, "lichess_since_testuser").as_deref(),
        Some((NEWEST_MS + 1).to_string().as_str()),
        "empty export leaves the cursor untouched"
    );
}

#[test]
fn unknown_user_404_is_an_error() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&lichess::export_url("nosuchuser", None), Scripted::NotFound);
    let err = lichess::sync_user(&conn, &fetcher, "nosuchuser").unwrap_err();
    assert!(err.to_string().contains("404"), "got: {err}");
}

/// LIVE NETWORK TEST — runs only with KIBITZ_NET_TESTS=1. Issues a single
/// tiny request (one game) against the real Lichess API.
#[test]
fn live_lichess_single_game_export() {
    if !live_tests_enabled("live_lichess_single_game_export") {
        return;
    }
    use kibitz_db::net::{FetchOutcome, Fetcher, UreqFetcher};
    use std::io::Read;
    let outcome = UreqFetcher
        .get(
            "https://lichess.org/api/games/user/thibault?max=1&pgnInJson=false\
             &clocks=false&evals=false&opening=false",
            &[("Accept", "application/x-chess-pgn")],
        )
        .unwrap();
    match outcome {
        FetchOutcome::Body(mut body) => {
            let mut text = String::new();
            body.read_to_string(&mut text).unwrap();
            assert!(text.contains("[Event "), "expected PGN, got: {text:.100}");
        }
        FetchOutcome::NotFound => panic!("thibault should exist"),
        FetchOutcome::RateLimited { .. } => eprintln!("rate limited; treating as pass"),
    }
}
