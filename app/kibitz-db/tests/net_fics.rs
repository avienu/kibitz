//! Offline tests for the ficsgames.org client. There is deliberately NO
//! live test: ficsgames.org is a volunteer, bandwidth-limited service whose
//! robots.txt discourages automated hits on the download CGI, so we only
//! exercise the client against fixtures.

mod common;

use common::{open_temp_db, FixtureFetcher, Scripted};
use kibitz_db::net::fics::{self, ResponseKind};

const FIXTURE_PGN: &[u8] = include_bytes!("fixtures/fics_testuser_2005.pgn");
const FIXTURE_ZIP: &[u8] = include_bytes!("fixtures/fics_testuser_2005.zip");

fn post_key() -> String {
    format!("POST {}", fics::DOWNLOAD_CGI)
}

#[test]
fn form_params_match_the_website_form() {
    let params = fics::form_params("testuser", 2005, None, false);
    assert_eq!(
        params,
        vec![
            ("gametype".to_string(), "11".to_string()),
            ("player".to_string(), "testuser".to_string()),
            ("year".to_string(), "2005".to_string()),
            ("month".to_string(), "0".to_string()),
            ("movetimes".to_string(), "0".to_string()),
            ("download".to_string(), "Download".to_string()),
        ]
    );
    let with_month = fics::form_params("u", 2010, Some(7), true);
    assert!(with_month.contains(&("month".to_string(), "7".to_string())));
    assert!(with_month.contains(&("movetimes".to_string(), "1".to_string())));
}

#[test]
fn classify_by_magic_bytes() {
    assert_eq!(fics::classify(b"PK\x03\x04rest"), ResponseKind::Zip);
    assert_eq!(fics::classify(b"BZh91AY&SY"), ResponseKind::Bzip2);
    assert_eq!(fics::classify(b"\n[Event \"x\"]"), ResponseKind::Pgn);
    assert_eq!(fics::classify(b"<html><body>error"), ResponseKind::Unknown);
    assert_eq!(fics::classify(b""), ResponseKind::Unknown);
}

#[test]
fn zip_response_is_unpacked_and_imported() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&post_key(), Scripted::Body(FIXTURE_ZIP.to_vec()));

    let report = fics::sync_user(&conn, &fetcher, "testuser", 2005, None).unwrap();
    assert_eq!(report.games_imported, 2);
    assert_eq!(report.saved_archive, None);

    // Exactly one serial POST, with the website's form fields.
    assert_eq!(fetcher.requested_urls(), vec![post_key()]);
    let form = fetcher.headers_of(0);
    assert!(form.contains(&("gametype".to_string(), "11".to_string())));
    assert!(form.contains(&("player".to_string(), "testuser".to_string())));

    // Provenance recorded.
    let (origin, license): (String, String) = conn
        .query_row(
            "SELECT origin, license FROM sources WHERE name LIKE 'FICS: testuser 2005%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(origin.contains("download.cgi"));
    assert!(origin.contains("player=testuser"));
    assert_eq!(license, fics::FICS_LICENSE);
}

#[test]
fn plain_pgn_response_is_imported_directly() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&post_key(), Scripted::Body(FIXTURE_PGN.to_vec()));
    let report = fics::sync_user(&conn, &fetcher, "testuser", 2005, Some(6)).unwrap();
    assert_eq!(report.games_imported, 2);
    assert_eq!(report.month, Some(6));
}

#[test]
fn bzip2_response_is_saved_for_manual_decompression() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    // Not valid bzip2 beyond the magic — the client must only classify+save.
    fetcher.script(&post_key(), Scripted::Body(b"BZh91AY&SYfakedata".to_vec()));
    let report = fics::sync_user(&conn, &fetcher, "testuser", 2005, None).unwrap();
    assert_eq!(report.games_imported, 0);
    let path = report.saved_archive.expect("bz2 must be saved");
    assert!(path.to_string_lossy().ends_with(".pgn.bz2"));
    assert_eq!(std::fs::read(&path).unwrap(), b"BZh91AY&SYfakedata");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn html_error_page_is_a_clear_error() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(
        &post_key(),
        Scripted::Body(b"<html>No games found for that player</html>".to_vec()),
    );
    let err = fics::sync_user(&conn, &fetcher, "nosuchplayer", 2005, None).unwrap_err();
    assert!(err.to_string().contains("No games found"), "got: {err}");
}

#[test]
fn month_out_of_range_is_rejected_before_any_request() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    let err = fics::sync_user(&conn, &fetcher, "u", 2005, Some(13)).unwrap_err();
    assert!(err.to_string().contains("month"), "got: {err}");
    assert!(fetcher.requested_urls().is_empty());
}
