//! Offline tests for the shared 429 backoff policy: the pure decision
//! function and the retry driver with an injected sleeper (no real waiting,
//! no network).

mod common;

use std::io::Read;
use std::time::Duration;

use common::{FixtureFetcher, Scripted};
use silman_db::net::{backoff_delay, fetch_with_retry, DEFAULT_BACKOFF_SECS};

#[test]
fn backoff_delay_honors_retry_after_and_falls_back_to_60s() {
    assert_eq!(
        backoff_delay(Some(30), 1, 4),
        Some(Duration::from_secs(30)),
        "server's Retry-After wins"
    );
    assert_eq!(
        backoff_delay(None, 1, 4),
        Some(Duration::from_secs(DEFAULT_BACKOFF_SECS)),
        "no Retry-After: back off 60s"
    );
    assert_eq!(backoff_delay(Some(30), 4, 4), None, "abort at max failures");
    assert_eq!(backoff_delay(None, 5, 4), None);
}

#[test]
fn fetch_with_retry_sleeps_then_succeeds() {
    let fetcher = FixtureFetcher::new();
    let url = "https://example.test/resource";
    fetcher.script(url, Scripted::RateLimited(Some(2)));
    fetcher.script(url, Scripted::RateLimited(None));
    fetcher.script(url, Scripted::Body(b"payload".to_vec()));

    let mut sleeps = Vec::new();
    let body = fetch_with_retry(&fetcher, url, &[], 4, &mut |d| sleeps.push(d))
        .unwrap()
        .expect("third attempt succeeds");
    let mut text = String::new();
    body.take(1024).read_to_string(&mut text).unwrap();
    assert_eq!(text, "payload");
    assert_eq!(
        sleeps,
        vec![
            Duration::from_secs(2),
            Duration::from_secs(DEFAULT_BACKOFF_SECS)
        ],
        "Retry-After honored, then the 60s fallback"
    );
    assert_eq!(fetcher.requested_urls().len(), 3, "strictly serial retries");
}

#[test]
fn fetch_with_retry_aborts_after_max_failures() {
    let fetcher = FixtureFetcher::new();
    let url = "https://example.test/limited";
    for _ in 0..3 {
        fetcher.script(url, Scripted::RateLimited(Some(1)));
    }
    let mut sleeps = Vec::new();
    let err = match fetch_with_retry(&fetcher, url, &[], 2, &mut |d| sleeps.push(d)) {
        Err(e) => e,
        Ok(_) => panic!("expected the fetch to abort"),
    };
    assert!(err.to_string().contains("fetching"), "got: {err}");
    assert!(format!("{err:#}").contains("rate-limit"), "got: {err:#}");
    assert_eq!(sleeps.len(), 1, "slept once, aborted on the second 429");
    assert_eq!(fetcher.requested_urls().len(), 2);
}

#[test]
fn fetch_with_retry_passes_404_through_as_none() {
    let fetcher = FixtureFetcher::new();
    let url = "https://example.test/missing";
    fetcher.script(url, Scripted::NotFound);
    let got = fetch_with_retry(&fetcher, url, &[], 4, &mut |_| ()).unwrap();
    assert!(got.is_none());
}
