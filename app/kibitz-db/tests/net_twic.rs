//! Offline tests for the TWIC incremental ingester (fixture zips only —
//! TWIC's bandwidth is donation-funded, so there is deliberately NO live
//! network test for TWIC).

mod common;

use common::{open_temp_db, FixtureFetcher, Scripted};
use kibitz_db::twic::{self, TwicOptions};

const ZIP_A: &[u8] = include_bytes!("fixtures/twic_a.zip"); // 2 games
const ZIP_B: &[u8] = include_bytes!("fixtures/twic_b.zip"); // 1 game

#[test]
fn zip_url_format() {
    assert_eq!(
        twic::zip_url(1580),
        "https://theweekinchess.com/zips/twic1580g.zip"
    );
}

#[test]
fn first_run_requires_explicit_from() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    let err = twic::sync(&conn, &fetcher, &TwicOptions::default()).unwrap_err();
    assert!(err.to_string().contains("--from"), "got: {err}");
    assert!(
        fetcher.requested_urls().is_empty(),
        "must not touch network"
    );
}

#[test]
fn first_run_imports_serially_until_404() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1500), Scripted::Body(ZIP_A.to_vec()));
    fetcher.script(&twic::zip_url(1501), Scripted::Body(ZIP_B.to_vec()));
    fetcher.script(&twic::zip_url(1502), Scripted::NotFound);

    let opts = TwicOptions {
        from: Some(1500),
        ..TwicOptions::default()
    };
    let report = twic::sync(&conn, &fetcher, &opts).unwrap();

    assert_eq!(report.issues.len(), 2);
    assert_eq!(report.issues[0].issue, 1500);
    assert_eq!(report.issues[0].games_imported, 2);
    assert_eq!(report.issues[1].issue, 1501);
    assert_eq!(report.issues[1].games_imported, 1);
    assert!(report.up_to_date, "stopped because 1502 returned 404");

    // First-run notice with the exact pointers the CLI must print.
    let notice = report.first_run_notice.expect("first run sets the notice");
    assert_eq!(notice, twic::FIRST_RUN_NOTICE);
    assert!(notice.contains("https://theweekinchess.com/twic"));
    assert!(notice.to_lowercase().contains("donation"));

    // Requests were serial and in ascending-issue order.
    assert_eq!(
        fetcher.requested_urls(),
        vec![
            twic::zip_url(1500),
            twic::zip_url(1501),
            twic::zip_url(1502)
        ]
    );

    // twic_issues bookkeeping.
    assert_eq!(twic::latest_imported(&conn).unwrap(), Some(1501));
    let (games_1500, source_id): (i64, i64) = conn
        .query_row(
            "SELECT games, source_id FROM twic_issues WHERE issue = 1500",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(games_1500, 2);

    // Provenance recorded on the source row.
    let (name, origin, license): (String, String, String) = conn
        .query_row(
            "SELECT name, origin, license FROM sources WHERE id = ?1",
            [source_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(name, "TWIC 1500");
    assert_eq!(origin, twic::zip_url(1500));
    assert_eq!(license, "TWIC personal use — not redistributable");

    let games: i64 = conn
        .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
        .unwrap();
    assert_eq!(games, 3);
}

#[test]
fn resume_skips_imported_issues_and_honors_max_cap_and_dup_detection() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1500), Scripted::Body(ZIP_A.to_vec()));
    fetcher.script(&twic::zip_url(1501), Scripted::NotFound);
    let opts = TwicOptions {
        from: Some(1500),
        ..TwicOptions::default()
    };
    twic::sync(&conn, &fetcher, &opts).unwrap();

    // Second run: resumes at 1501 (ignores `from`), capped at one issue.
    // Issue 1501's zip here contains the same games as 1500 → all dups.
    let fetcher2 = FixtureFetcher::new();
    fetcher2.script(&twic::zip_url(1501), Scripted::Body(ZIP_A.to_vec()));
    let opts2 = TwicOptions {
        from: Some(999), // must be ignored on resume
        max_issues: 1,
    };
    let report = twic::sync(&conn, &fetcher2, &opts2).unwrap();

    assert_eq!(fetcher2.requested_urls(), vec![twic::zip_url(1501)]);
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].issue, 1501);
    assert_eq!(report.issues[0].games_imported, 0);
    assert_eq!(report.issues[0].duplicates_skipped, 2);
    assert!(!report.up_to_date, "cap reached; more issues may exist");
    assert!(report.first_run_notice.is_none(), "not a first run");
    assert_eq!(twic::latest_imported(&conn).unwrap(), Some(1501));
}

#[test]
fn default_max_issues_is_five() {
    assert_eq!(TwicOptions::default().max_issues, 5);
}

#[test]
fn corrupt_zip_is_an_error() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(42), Scripted::Body(b"not a zip".to_vec()));
    let opts = TwicOptions {
        from: Some(42),
        ..TwicOptions::default()
    };
    let err = twic::sync(&conn, &fetcher, &opts).unwrap_err();
    assert!(err.to_string().contains("unzipping"), "got: {err:#}");
    // Nothing recorded for the failed issue.
    assert_eq!(twic::latest_imported(&conn).unwrap(), None);
}
