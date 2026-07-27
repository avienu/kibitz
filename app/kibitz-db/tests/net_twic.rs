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

// ---------------------------------------------------------------------------
// import_issue (run 9: the UI catalog's per-issue download path)
// ---------------------------------------------------------------------------

#[test]
fn import_issue_records_provenance_and_refuses_refetch() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1500), Scripted::Body(ZIP_A.to_vec()));

    let report = twic::import_issue(&conn, &fetcher, 1500)
        .unwrap()
        .expect("issue exists");
    assert_eq!(report.issue, 1500);
    assert_eq!(report.games_imported, 2);
    assert_eq!(twic::imported_issues(&conn).unwrap(), vec![(1500, 2)]);

    // Refetching an imported issue is refused BEFORE touching the network.
    let err = twic::import_issue(&conn, &fetcher, 1500).unwrap_err();
    assert!(
        err.to_string().contains("never fetched twice"),
        "got: {err}"
    );
    assert_eq!(fetcher.requested_urls().len(), 1, "no second request");
}

#[test]
fn import_issue_returns_none_on_404() {
    let (_dir, conn) = open_temp_db();
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(999), Scripted::NotFound);
    assert!(twic::import_issue(&conn, &fetcher, 999).unwrap().is_none());
    assert_eq!(twic::latest_imported(&conn).unwrap(), None);
}

// ---------------------------------------------------------------------------
// probe_latest (run 9: explicit "Refresh catalog" only — bounded HEADs)
// ---------------------------------------------------------------------------

fn no_sleep() -> impl FnMut(std::time::Duration) {
    |_| panic!("probe must not sleep in these scenarios")
}

#[test]
fn probe_accurate_guess_costs_two_requests() {
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1650), Scripted::Body(vec![]));
    fetcher.script(&twic::zip_url(1651), Scripted::NotFound);
    let r = twic::probe_latest(&fetcher, Some(1600), 1650, &mut no_sleep()).unwrap();
    assert_eq!(r.latest, Some(1650));
    assert_eq!(r.requests, 2, "the documented typical cost");
}

#[test]
fn probe_low_guess_walks_forward_to_the_first_404() {
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1650), Scripted::Body(vec![]));
    fetcher.script(&twic::zip_url(1651), Scripted::Body(vec![]));
    fetcher.script(&twic::zip_url(1652), Scripted::NotFound);
    let r = twic::probe_latest(&fetcher, None, 1650, &mut no_sleep()).unwrap();
    assert_eq!(r.latest, Some(1651));
    assert_eq!(r.requests, 3);
}

#[test]
fn probe_high_guess_walks_backward_to_the_first_hit() {
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1655), Scripted::NotFound);
    fetcher.script(&twic::zip_url(1654), Scripted::NotFound);
    fetcher.script(&twic::zip_url(1653), Scripted::Body(vec![]));
    let r = twic::probe_latest(&fetcher, Some(1600), 1655, &mut no_sleep()).unwrap();
    assert_eq!(r.latest, Some(1653));
    assert_eq!(r.requests, 3);
}

#[test]
fn probe_falls_back_to_the_floor_without_reverifying_it() {
    // Everything above the floor 404s; the floor itself (an imported
    // issue, known to exist) is returned without a request for it.
    let fetcher = FixtureFetcher::new();
    for issue in 1601..=1603 {
        fetcher.script(&twic::zip_url(issue), Scripted::NotFound);
    }
    let r = twic::probe_latest(&fetcher, Some(1600), 1603, &mut no_sleep()).unwrap();
    assert_eq!(r.latest, Some(1600));
    assert_eq!(r.requests, 3);
    assert!(
        !fetcher.requested_urls().contains(&twic::zip_url(1600)),
        "the floor is trusted, not probed"
    );
}

#[test]
fn probe_request_cap_is_honored() {
    // No floor, guess just above the archive start, nothing exists:
    // the walk stops at FIRST_AVAILABLE_ISSUE, all misses.
    let fetcher = FixtureFetcher::new();
    let first = twic::FIRST_AVAILABLE_ISSUE;
    for issue in first..=first + 5 {
        fetcher.script(&twic::zip_url(issue), Scripted::NotFound);
    }
    let r = twic::probe_latest(&fetcher, None, first + 5, &mut no_sleep()).unwrap();
    assert_eq!(r.latest, None);
    assert!(
        r.requests <= twic::PROBE_MAX_REQUESTS,
        "requests {} exceed the documented cap",
        r.requests
    );
}

#[test]
fn probe_respects_429_backoff() {
    let fetcher = FixtureFetcher::new();
    fetcher.script(&twic::zip_url(1650), Scripted::RateLimited(Some(7)));
    fetcher.script(&twic::zip_url(1650), Scripted::Body(vec![]));
    fetcher.script(&twic::zip_url(1651), Scripted::NotFound);
    let mut slept = Vec::new();
    let r = twic::probe_latest(&fetcher, None, 1650, &mut |d| slept.push(d)).unwrap();
    assert_eq!(r.latest, Some(1650));
    assert_eq!(slept, vec![std::time::Duration::from_secs(7)]);
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
