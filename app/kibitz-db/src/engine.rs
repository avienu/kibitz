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

/// KIBITZ_STOCKFISH > repo-local tools/ binary > `stockfish` on PATH.
pub fn resolve_engine_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KIBITZ_STOCKFISH") {
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

/// One iteration of a running search, as reported to a watcher.
pub struct SearchTick<'a> {
    /// Depth this `info` line reports (not the target depth).
    pub depth: u32,
    pub nodes: u64,
    pub nps: u64,
    /// Best line per MultiPV slot so far, best first.
    pub lines: &'a [EngineLine],
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

    /// `go depth D` on `fen` with `MultiPV = multipv`, returning up to
    /// `multipv` candidate lines, best first (evals from the side to
    /// move's point of view). MultiPV is reset to 1 afterwards so the
    /// single-line searches other job kinds run stay unaffected.
    pub fn eval_depth_multipv(
        &mut self,
        fen: &str,
        multipv: u32,
        depth: u32,
    ) -> anyhow::Result<Vec<EngineLine>> {
        self.eval_depth_multipv_watched(fen, multipv, depth, &mut |_| {})
    }

    /// [`eval_depth_multipv`] reporting each iteration as it lands, so a
    /// caller can show the search working instead of a spinner. `on_tick`
    /// fires once per `info` line that carries a PV — often, and from the
    /// search thread: throttle in the callback, do no real work there.
    pub fn eval_depth_multipv_watched(
        &mut self,
        fen: &str,
        multipv: u32,
        depth: u32,
        on_tick: &mut dyn FnMut(SearchTick<'_>),
    ) -> anyhow::Result<Vec<EngineLine>> {
        let multipv = multipv.max(1);
        self.send("ucinewgame")?;
        self.send(&format!("setoption name MultiPV value {multipv}"))?;
        self.send(&format!("position fen {fen}"))?;
        self.send(&format!("go depth {depth}"))?;
        // Slot per MultiPV index; each deeper iteration overwrites the
        // shallower one, so what remains is the deepest line per slot.
        let mut lines: Vec<Option<EngineLine>> = vec![None; multipv as usize];
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                anyhow::bail!("engine closed stdout during search");
            }
            let l = line.trim();
            if l.starts_with("info ") && l.contains(" pv ") {
                let mut idx: usize = 1;
                let mut score_cp = None;
                let mut mate = None;
                let mut pv = vec![];
                let mut at_depth = 0;
                let mut nodes = 0;
                let mut nps = 0;
                let mut it = l.split_whitespace().peekable();
                while let Some(tok) = it.next() {
                    match tok {
                        "depth" => at_depth = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
                        "nodes" => nodes = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
                        "nps" => nps = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
                        "multipv" => {
                            idx = it.next().and_then(|v| v.parse().ok()).unwrap_or(1);
                        }
                        "cp" => score_cp = it.next().and_then(|v| v.parse().ok()),
                        "mate" => mate = it.next().and_then(|v| v.parse().ok()),
                        "pv" => {
                            pv = it.by_ref().map(str::to_string).collect();
                        }
                        _ => {}
                    }
                }
                if !pv.is_empty() && (1..=multipv as usize).contains(&idx) {
                    lines[idx - 1] = Some(EngineLine {
                        score_cp: score_cp.unwrap_or_else(|| {
                            mate.map(|m: i32| if m > 0 { 10_000 } else { -10_000 })
                                .unwrap_or(0)
                        }),
                        mate,
                        pv,
                    });
                    let so_far: Vec<EngineLine> = lines.iter().flatten().cloned().collect();
                    on_tick(SearchTick {
                        depth: at_depth,
                        nodes,
                        nps,
                        lines: &so_far,
                    });
                }
            } else if l.starts_with("bestmove") {
                break;
            }
        }
        self.send("setoption name MultiPV value 1")?;
        Ok(lines.into_iter().flatten().collect())
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
