//! Lichess opening-explorer proxy (run 10, app layer — CLAUDE.md keeps
//! all network out of the BSD crates).
//!
//! One command: fetch the explorer JSON for a FEN and hand the body to
//! the frontend verbatim. Proxying through Rust (kibitz-db's net
//! plumbing: `UreqFetcher`, descriptive User-Agent) sidesteps webview
//! CORS/CSP variance across platforms and keeps a single code path in
//! dev and packaged builds.
//!
//! Network-quiet by default: nothing here runs unless the user switched
//! the Opening-tree screen's online toggle ON (OFF by default, frontend
//! enforced); the frontend additionally debounces (500 ms) and caches
//! per FEN, so at most one request per settled position.

use cozy_chess::Board;
use kibitz_db::net::{FetchOutcome, Fetcher, UreqFetcher};
use std::io::Read;

/// Refuse to buffer more than this much response body (the explorer's
/// replies are a few KB; anything bigger is not the API we asked for).
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;

/// Build the explorer URL for a FEN. Only the characters a FEN can
/// contain need escaping in a query value: '/' and ' '.
pub(crate) fn explorer_url(fen: &str) -> String {
    let encoded = fen.replace('/', "%2F").replace(' ', "%20");
    format!("https://explorer.lichess.ovh/lichess?variant=standard&fen={encoded}")
}

/// Fetch via any [`Fetcher`] — injectable so tests stay fully offline.
pub(crate) fn fetch_via(fetcher: &dyn Fetcher, fen: &str) -> Result<String, String> {
    // Sanity-parse before spending a network request on junk.
    let _board: Board = fen
        .parse()
        .map_err(|e| format!("not a valid FEN: {e:?}"))?;
    let url = explorer_url(fen);
    match fetcher.get(&url, &[("Accept", "application/json")]) {
        Ok(FetchOutcome::Body(body)) => {
            let mut out = String::new();
            body.take(MAX_BODY_BYTES)
                .read_to_string(&mut out)
                .map_err(|e| format!("reading explorer response: {e}"))?;
            Ok(out)
        }
        Ok(FetchOutcome::NotFound) => Err("lichess explorer: not found (HTTP 404)".to_string()),
        Ok(FetchOutcome::RateLimited { retry_after_secs }) => Err(format!(
            "rate-limited by lichess — try again in {} s",
            retry_after_secs.unwrap_or(60)
        )),
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Raw explorer JSON for `fen`. Only ever called after the user opted
/// into online data (the frontend toggle); rate-limit and offline
/// failures surface as honest error strings, never crashes.
#[tauri::command]
pub async fn explorer_fetch(fen: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_via(&UreqFetcher, &fen))
        .await
        .map_err(|e| format!("explorer task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    struct FixtureFetcher(FetchOutcome);
    impl Fetcher for FixtureFetcher {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
            assert!(url.starts_with("https://explorer.lichess.ovh/lichess?"));
            assert!(headers.contains(&("Accept", "application/json")));
            match &self.0 {
                FetchOutcome::Body(_) => Ok(FetchOutcome::Body(Box::new(Cursor::new(
                    br#"{"white":1,"draws":0,"black":0,"moves":[]}"#.to_vec(),
                )))),
                FetchOutcome::NotFound => Ok(FetchOutcome::NotFound),
                FetchOutcome::RateLimited { retry_after_secs } => Ok(FetchOutcome::RateLimited {
                    retry_after_secs: *retry_after_secs,
                }),
            }
        }
        fn post_form(&self, _: &str, _: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
            unreachable!("explorer never POSTs")
        }
    }

    #[test]
    fn url_escapes_the_fen_and_targets_the_lichess_db() {
        let url = explorer_url(START);
        assert_eq!(
            url,
            "https://explorer.lichess.ovh/lichess?variant=standard&fen=rnbqkbnr%2Fpppppppp%2F8%2F8%2F8%2F8%2FPPPPPPPP%2FRNBQKBNR%20w%20KQkq%20-%200%201"
        );
        assert!(!url.contains(' ') && !url.contains('/') || url.starts_with("https://"));
    }

    #[test]
    fn body_passes_through_verbatim() {
        let body = FixtureFetcher(FetchOutcome::Body(Box::new(Cursor::new(Vec::new()))));
        let out = fetch_via(&body, START).unwrap();
        assert_eq!(out, r#"{"white":1,"draws":0,"black":0,"moves":[]}"#);
    }

    #[test]
    fn rate_limits_and_404s_become_honest_errors_and_bad_fens_never_hit_the_network() {
        let rl = FixtureFetcher(FetchOutcome::RateLimited {
            retry_after_secs: Some(30),
        });
        assert_eq!(
            fetch_via(&rl, START).unwrap_err(),
            "rate-limited by lichess — try again in 30 s"
        );
        let nf = FixtureFetcher(FetchOutcome::NotFound);
        assert!(fetch_via(&nf, START).unwrap_err().contains("404"));

        struct Panicker;
        impl Fetcher for Panicker {
            fn get(&self, _: &str, _: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
                panic!("a bad FEN must be rejected before any request")
            }
            fn post_form(&self, _: &str, _: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
                unreachable!()
            }
        }
        assert!(fetch_via(&Panicker, "not a fen")
            .unwrap_err()
            .contains("not a valid FEN"));
    }
}
