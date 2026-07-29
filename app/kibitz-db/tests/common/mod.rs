//! Shared offline-test helpers: a scripted [`Fetcher`] and a temp database.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;

use kibitz_db::net::{FetchOutcome, Fetcher};

/// One scripted response for a URL.
pub enum Scripted {
    Body(Vec<u8>),
    /// A body whose read fails after the prefix — a stalled/severed
    /// connection surfacing as a read timeout mid-stream.
    BrokenBody(Vec<u8>),
    NotFound,
    RateLimited(Option<u64>),
}

/// Reader that yields `prefix` then errors — the shape of a timed-out
/// download once the fetcher has real read timeouts.
struct BrokenReader {
    prefix: Cursor<Vec<u8>>,
    done: bool,
}

impl std::io::Read for BrokenReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.prefix.read(buf)?;
        if n > 0 {
            return Ok(n);
        }
        if self.done {
            return Ok(0);
        }
        self.done = true;
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "simulated mid-body read timeout",
        ))
    }
}

/// A logged request: the key (URL, or `POST {url}`) and its header pairs
/// (form pairs for POSTs).
type LoggedRequest = (String, Vec<(String, String)>);

/// Offline [`Fetcher`]: replays scripted responses per URL (FIFO per URL)
/// and records every request (URL + headers) for assertions. Any request
/// for an unscripted URL is an error, so tests fail loudly instead of
/// silently hitting the network path.
#[derive(Default)]
pub struct FixtureFetcher {
    responses: RefCell<HashMap<String, VecDeque<Scripted>>>,
    log: RefCell<Vec<LoggedRequest>>,
}

impl FixtureFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn script(&self, url: &str, response: Scripted) {
        self.responses
            .borrow_mut()
            .entry(url.to_string())
            .or_default()
            .push_back(response);
    }

    /// URLs requested, in order.
    pub fn requested_urls(&self) -> Vec<String> {
        self.log.borrow().iter().map(|(u, _)| u.clone()).collect()
    }

    /// Headers sent with the `n`-th request.
    pub fn headers_of(&self, n: usize) -> Vec<(String, String)> {
        self.log.borrow()[n].1.clone()
    }
}

impl FixtureFetcher {
    fn respond(&self, key: &str, pairs: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        self.log.borrow_mut().push((
            key.to_string(),
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ));
        let mut map = self.responses.borrow_mut();
        let queue = map
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("unscripted request: {key}"))?;
        let scripted = queue
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("no scripted responses left for {key}"))?;
        Ok(match scripted {
            Scripted::Body(bytes) => FetchOutcome::Body(Box::new(Cursor::new(bytes))),
            Scripted::BrokenBody(prefix) => FetchOutcome::Body(Box::new(BrokenReader {
                prefix: Cursor::new(prefix),
                done: false,
            })),
            Scripted::NotFound => FetchOutcome::NotFound,
            Scripted::RateLimited(retry_after_secs) => {
                FetchOutcome::RateLimited { retry_after_secs }
            }
        })
    }
}

impl Fetcher for FixtureFetcher {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        self.respond(url, headers)
    }

    /// POSTs are scripted and logged under the key `POST {url}`, with the
    /// form pairs recorded where GET records headers.
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> anyhow::Result<FetchOutcome> {
        self.respond(&format!("POST {url}"), form)
    }
}

/// Fresh migrated database in a temp dir (keep the `TempDir` alive).
pub fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = kibitz_db::db::open(&dir.path().join("test.sqlite")).unwrap();
    (dir, conn)
}

/// Read a meta-table value directly, for bookkeeping assertions.
pub fn meta(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

/// True when live-network tests are enabled (KIBITZ_NET_TESTS=1).
pub fn live_tests_enabled(name: &str) -> bool {
    if std::env::var("KIBITZ_NET_TESTS").as_deref() == Ok("1") {
        true
    } else {
        eprintln!("skipping live network test {name}; set KIBITZ_NET_TESTS=1 to run");
        false
    }
}
