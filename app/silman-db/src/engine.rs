//! Blocking UCI engine client for batch jobs (the interactive app uses the
//! tokio manager in src-tauri). Every spawn increments [`SPAWN_COUNT`] so
//! tests can assert the engine-off product principle (CLAUDE.md #6).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Total engine processes spawned by this module in this process.
pub static SPAWN_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn spawn_count() -> u64 {
    SPAWN_COUNT.load(Ordering::SeqCst)
}

/// SILMAN_STOCKFISH > repo-local tools/ binary > `stockfish` on PATH.
pub fn resolve_engine_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SILMAN_STOCKFISH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    // Walk up from cwd looking for the dev binary.
    let rel = Path::new("tools/stockfish/stockfish-macos-m1-apple-silicon");
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join(rel);
        if cand.is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            break;
        }
    }
    which_stockfish()
}

fn which_stockfish() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("stockfish"))
        .find(|p| p.is_file())
}

pub struct Engine {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The engine's own `id name` string (verdict 3a: stamped on every
    /// stored analysis).
    pub identity: String,
}

#[derive(Debug, Clone)]
pub struct EngineLine {
    pub score_cp: i32,
    pub mate: Option<i32>,
    pub pv: Vec<String>,
}

impl Engine {
    pub fn spawn(path: &Path) -> anyhow::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        SPAWN_COUNT.fetch_add(1, Ordering::SeqCst);
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut engine = Self {
            child,
            stdin,
            stdout,
            identity: String::new(),
        };
        engine.send("uci")?;
        engine.handshake()?;
        Ok(engine)
    }

    /// Read until `uciok`, capturing the `id name` line.
    fn handshake(&mut self) -> anyhow::Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                anyhow::bail!("engine closed stdout during handshake");
            }
            let l = line.trim();
            if let Some(name) = l.strip_prefix("id name ") {
                self.identity = name.trim().to_string();
            }
            if l.starts_with("uciok") {
                if self.identity.is_empty() {
                    self.identity = "unknown engine".to_string();
                }
                return Ok(());
            }
        }
    }

    fn send(&mut self, line: &str) -> anyhow::Result<()> {
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()?;
        Ok(())
    }

    /// `go nodes N` on `fen`, returning the best line (from the side to
    /// move's point of view).
    pub fn eval_nodes(&mut self, fen: &str, nodes: u64) -> anyhow::Result<EngineLine> {
        self.send("ucinewgame")?;
        self.send(&format!("position fen {fen}"))?;
        self.send(&format!("go nodes {nodes}"))?;
        let mut best = EngineLine {
            score_cp: 0,
            mate: None,
            pv: vec![],
        };
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                anyhow::bail!("engine closed stdout during search");
            }
            let l = line.trim();
            if l.starts_with("info ") && l.contains(" pv ") {
                let mut score_cp = None;
                let mut mate = None;
                let mut pv = vec![];
                let mut it = l.split_whitespace().peekable();
                while let Some(tok) = it.next() {
                    match tok {
                        "cp" => score_cp = it.next().and_then(|v| v.parse().ok()),
                        "mate" => mate = it.next().and_then(|v| v.parse().ok()),
                        "pv" => {
                            pv = it.by_ref().map(str::to_string).collect();
                        }
                        _ => {}
                    }
                }
                if !pv.is_empty() {
                    best = EngineLine {
                        score_cp: score_cp.unwrap_or_else(|| {
                            mate.map(|m: i32| if m > 0 { 10_000 } else { -10_000 })
                                .unwrap_or(0)
                        }),
                        mate,
                        pv,
                    };
                }
            } else if l.starts_with("bestmove") {
                return Ok(best);
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send("quit");
        let _ = self.child.wait();
    }
}
