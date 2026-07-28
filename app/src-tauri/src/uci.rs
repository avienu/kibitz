//! UCI subprocess manager (Phase 0 spike).
//!
//! Spawns a UCI engine (Stockfish) as a child process, speaks the UCI line
//! protocol over stdin/stdout via tokio, and parses `info` / `bestmove`
//! output into typed structs. This module is deliberately self-contained:
//! it is the seed of the engine job-queue manager described in
//! docs/ARCHITECTURE.md and must stay free of Tauri types.
//!
//! Product principle (CLAUDE.md #6): nothing in this module starts an engine
//! on its own; callers (UI commands, later the job queue) decide when to run.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// Default node budget for an analysis run (`go nodes N`).
pub const DEFAULT_NODES: u64 = 2_000_000;

/// Relative path (from the repo root) of the bundled dev engine binary.
const REPO_ENGINE_RELPATH: &str = "tools/stockfish/stockfish-macos-m1-apple-silicon";

/// One parsed UCI `info` line. Field names are serialized in camelCase to
/// match the TypeScript `EngineInfo` interface (app/src/lib/engineView.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UciInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seldepth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multipv: Option<u32>,
    /// Centipawn score, side-to-move POV (UCI convention).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_cp: Option<i32>,
    /// Mate in N (negative: getting mated), side-to-move POV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_mate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<u64>,
    /// Principal variation as UCI move strings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pv: Option<Vec<String>>,
}

impl UciInfo {
    /// True if this line carries an evaluation worth showing (filters out
    /// `currmove` progress lines and `info string` chatter).
    pub fn has_score(&self) -> bool {
        self.score_cp.is_some() || self.score_mate.is_some()
    }
}

/// Parsed `bestmove` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BestMove {
    pub bestmove: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ponder: Option<String>,
}

/// Position to analyze, mapping onto the UCI `position` command.
#[derive(Debug, Clone)]
pub enum UciPosition {
    Startpos,
    Fen(String),
}

impl UciPosition {
    fn to_command(&self) -> String {
        match self {
            UciPosition::Startpos => "position startpos".to_owned(),
            UciPosition::Fen(fen) => format!("position fen {fen}"),
        }
    }
}

/// Parse a single UCI `info` line. Returns `None` for non-info lines.
///
/// Recognized tokens: depth, seldepth, multipv, score cp|mate (ignoring
/// lowerbound/upperbound qualifiers), nodes, nps, time, pv (rest of line).
pub fn parse_info_line(line: &str) -> Option<UciInfo> {
    let mut tokens = line.split_whitespace();
    if tokens.next()? != "info" {
        return None;
    }
    let mut info = UciInfo::default();
    let mut saw_field = false;
    while let Some(tok) = tokens.next() {
        match tok {
            "depth" => info.depth = tokens.next().and_then(|v| v.parse().ok()),
            "seldepth" => info.seldepth = tokens.next().and_then(|v| v.parse().ok()),
            "multipv" => info.multipv = tokens.next().and_then(|v| v.parse().ok()),
            "score" => match tokens.next() {
                Some("cp") => info.score_cp = tokens.next().and_then(|v| v.parse().ok()),
                Some("mate") => info.score_mate = tokens.next().and_then(|v| v.parse().ok()),
                _ => {}
            },
            "nodes" => info.nodes = tokens.next().and_then(|v| v.parse().ok()),
            "nps" => info.nps = tokens.next().and_then(|v| v.parse().ok()),
            "time" => info.time_ms = tokens.next().and_then(|v| v.parse().ok()),
            "pv" => {
                info.pv = Some(tokens.map(str::to_owned).collect());
                break;
            }
            "string" => return None, // `info string ...`: engine chatter, skip
            _ => continue,           // unknown/valueless token (lowerbound, currmove val, ...)
        }
        saw_field = true;
    }
    if saw_field {
        Some(info)
    } else {
        None
    }
}

/// Parse a UCI `id name <name>` handshake line. Returns `None` for
/// anything else (including a bare "id name" with no value).
pub fn parse_id_name(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("id name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Engine identity from a `uci` handshake (Settings' engine manager).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineIdentity {
    /// The binary actually spoken to (after resolution).
    pub path: String,
    /// The engine's `id name` line ("Stockfish 17.1"); None when the
    /// binary completed the handshake without sending one.
    pub name: Option<String>,
}

/// Spawn `path`, run the `uci` → `uciok` handshake capturing the `id name`
/// line, then quit. This validates a user-picked binary WITHOUT starting
/// any search — an explicit Settings action, not a background engine run.
pub async fn identify(path: &Path) -> Result<EngineIdentity, String> {
    let mut child = Command::new(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn engine {}: {e}", path.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Engine stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Engine stdout unavailable".to_owned())?;
    stdin
        .write_all(b"uci\n")
        .await
        .map_err(|e| format!("Failed to send 'uci': {e}"))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("Failed to flush 'uci': {e}"))?;

    let mut lines = BufReader::new(stdout).lines();
    let handshake = async {
        let mut name: Option<String> = None;
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("Engine read error: {e}"))?
        {
            if let Some(n) = parse_id_name(&line) {
                name = Some(n.to_owned());
            }
            if line.trim() == "uciok" {
                return Ok(name);
            }
        }
        Err("Engine closed stdout before 'uciok' — not a UCI engine?".to_owned())
    };
    let name = tokio::time::timeout(Duration::from_secs(10), handshake)
        .await
        .map_err(|_| "Timed out waiting for 'uciok' — not a UCI engine?".to_owned())??;
    let _ = stdin.write_all(b"quit\n").await;
    let _ = stdin.flush().await;
    Ok(EngineIdentity {
        path: path.display().to_string(),
        name,
    })
}

/// Parse a `bestmove <move> [ponder <move>]` line. Returns `None` otherwise.
pub fn parse_bestmove_line(line: &str) -> Option<BestMove> {
    let mut tokens = line.split_whitespace();
    if tokens.next()? != "bestmove" {
        return None;
    }
    let bestmove = tokens.next()?.to_owned();
    let ponder = match (tokens.next(), tokens.next()) {
        (Some("ponder"), Some(m)) => Some(m.to_owned()),
        _ => None,
    };
    Some(BestMove { bestmove, ponder })
}

/// Resolve which engine binary to use. Priority (first hit wins):
///
/// 1. Explicit user-set path (persisted in the UI). If set but missing,
///    this is a hard error — an explicit choice is never silently ignored.
/// 2. `KIBITZ_STOCKFISH` environment variable (skipped if stale/missing).
/// 3. `tools/stockfish/stockfish-macos-m1-apple-silicon` relative to the
///    repo root, found by walking up from the current dir and the exe dir.
/// 4. `stockfish` on `PATH`.
pub fn resolve_engine_path(user_path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = user_path.map(str::trim).filter(|p| !p.is_empty()) {
        let path = PathBuf::from(p);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("Configured engine path does not exist: {p}"))
        };
    }
    if let Some(p) = std::env::var_os("KIBITZ_STOCKFISH") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
        }
    }
    for root in &roots {
        for dir in root.ancestors() {
            let candidate = dir.join(REPO_ENGINE_RELPATH);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("stockfish");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(
        "No Stockfish binary found. Set an engine path in the UI, set KIBITZ_STOCKFISH, \
         or install stockfish on PATH."
            .to_owned(),
    )
}

/// Cloneable handle that can interrupt a running search out-of-band
/// (writes `stop` to the engine's stdin without waiting for the search lock).
#[derive(Clone)]
pub struct StopHandle {
    stdin: Arc<Mutex<ChildStdin>>,
}

impl StopHandle {
    /// Ask the engine to end the current search; it will reply `bestmove`.
    pub async fn stop(&self) -> Result<(), String> {
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(b"stop\n")
            .await
            .map_err(|e| format!("Failed to send stop: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stop: {e}"))
    }
}

/// A running UCI engine process after a successful `uci`/`isready` handshake.
pub struct Engine {
    path: PathBuf,
    _child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Engine {
    /// Spawn `path`, perform the `uci` → `uciok` and `isready` → `readyok`
    /// handshake (10s timeout), and return a ready engine.
    pub async fn spawn(path: &Path) -> Result<Self, String> {
        let mut child = Command::new(path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn engine {}: {e}", path.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Engine stdin unavailable".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Engine stdout unavailable".to_owned())?;
        let mut engine = Engine {
            path: path.to_path_buf(),
            _child: child,
            stdin: Arc::new(Mutex::new(stdin)),
            lines: BufReader::new(stdout).lines(),
        };
        engine.send("uci").await?;
        engine.wait_for("uciok", Duration::from_secs(10)).await?;
        engine.send("isready").await?;
        engine.wait_for("readyok", Duration::from_secs(10)).await?;
        Ok(engine)
    }

    /// The binary path this engine was spawned from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Handle for interrupting a search from another task.
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle {
            stdin: Arc::clone(&self.stdin),
        }
    }

    /// Write one UCI command line to the engine.
    pub async fn send(&self, cmd: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(format!("{cmd}\n").as_bytes())
            .await
            .map_err(|e| format!("Failed to send '{cmd}': {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush '{cmd}': {e}"))
    }

    /// Read stdout lines until one equals `expected` (or timeout).
    async fn wait_for(&mut self, expected: &str, timeout: Duration) -> Result<(), String> {
        let fut = async {
            while let Some(line) = self
                .lines
                .next_line()
                .await
                .map_err(|e| format!("Engine read error: {e}"))?
            {
                if line.trim() == expected {
                    return Ok(());
                }
            }
            Err(format!("Engine closed stdout before '{expected}'"))
        };
        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| format!("Timed out waiting for '{expected}'"))?
    }

    /// Run a search on `position`, invoking `on_info` for every parsed
    /// `info` line, until the engine reports `bestmove`. `nodes: Some(n)`
    /// runs `go nodes n`; `None` runs `go infinite` — which ends ONLY via
    /// [`Engine::stop_handle`] (live analysis, a deliberate user action).
    ///
    /// Either way the search can be interrupted early; the engine then
    /// emits `bestmove` promptly and this returns normally.
    pub async fn analyze(
        &mut self,
        position: &UciPosition,
        nodes: Option<u64>,
        mut on_info: impl FnMut(UciInfo),
    ) -> Result<BestMove, String> {
        self.send(&position.to_command()).await?;
        match nodes {
            Some(n) => self.send(&format!("go nodes {n}")).await?,
            None => self.send("go infinite").await?,
        }
        while let Some(line) = self
            .lines
            .next_line()
            .await
            .map_err(|e| format!("Engine read error: {e}"))?
        {
            if let Some(best) = parse_bestmove_line(&line) {
                return Ok(best);
            }
            if let Some(info) = parse_info_line(&line) {
                on_info(info);
            }
        }
        Err("Engine closed stdout during search".to_owned())
    }

    /// Politely shut the engine down (`quit`); the process is also killed on
    /// drop as a backstop (`kill_on_drop`).
    pub async fn quit(self) {
        let _ = self.send("quit").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_info_line() {
        let line = "info depth 20 seldepth 28 multipv 1 score cp 34 nodes 1234567 \
                    nps 2500000 hashfull 120 tbhits 0 time 494 pv e2e4 e7e5 g1f3";
        let info = parse_info_line(line).expect("should parse");
        assert_eq!(info.depth, Some(20));
        assert_eq!(info.seldepth, Some(28));
        assert_eq!(info.multipv, Some(1));
        assert_eq!(info.score_cp, Some(34));
        assert_eq!(info.score_mate, None);
        assert_eq!(info.nodes, Some(1_234_567));
        assert_eq!(info.nps, Some(2_500_000));
        assert_eq!(info.time_ms, Some(494));
        assert_eq!(
            info.pv.as_deref(),
            Some(&["e2e4".to_owned(), "e7e5".to_owned(), "g1f3".to_owned()][..])
        );
        assert!(info.has_score());
    }

    #[test]
    fn parses_mate_score_with_bound() {
        let info = parse_info_line("info depth 12 score mate -3 lowerbound nodes 999 pv h7h8q")
            .expect("should parse");
        assert_eq!(info.score_mate, Some(-3));
        assert_eq!(info.score_cp, None);
        assert!(info.has_score());
    }

    #[test]
    fn currmove_lines_have_no_score() {
        let info =
            parse_info_line("info depth 15 currmove e2e4 currmovenumber 1").expect("should parse");
        assert!(!info.has_score());
        assert_eq!(info.depth, Some(15));
    }

    #[test]
    fn skips_info_string_and_non_info_lines() {
        assert_eq!(
            parse_info_line("info string NNUE evaluation using nn.nnue"),
            None
        );
        assert_eq!(parse_info_line("id name Stockfish 18"), None);
        assert_eq!(parse_info_line("readyok"), None);
        assert_eq!(parse_info_line(""), None);
    }

    #[test]
    fn parses_bestmove_lines() {
        assert_eq!(
            parse_bestmove_line("bestmove e2e4 ponder e7e5"),
            Some(BestMove {
                bestmove: "e2e4".to_owned(),
                ponder: Some("e7e5".to_owned())
            })
        );
        assert_eq!(
            parse_bestmove_line("bestmove g1f3"),
            Some(BestMove {
                bestmove: "g1f3".to_owned(),
                ponder: None
            })
        );
        assert_eq!(parse_bestmove_line("info depth 1"), None);
    }

    #[test]
    fn position_command_forms() {
        assert_eq!(UciPosition::Startpos.to_command(), "position startpos");
        assert_eq!(
            UciPosition::Fen("8/8/8/8/8/8/8/K1k5 w - - 0 1".into()).to_command(),
            "position fen 8/8/8/8/8/8/8/K1k5 w - - 0 1"
        );
    }

    #[test]
    fn explicit_user_path_must_exist() {
        let err = resolve_engine_path(Some("/definitely/not/a/real/engine")).unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn parses_id_name_lines_and_rejects_the_rest() {
        assert_eq!(parse_id_name("id name Stockfish 17.1"), Some("Stockfish 17.1"));
        assert_eq!(parse_id_name("  id name Lc0 v0.31  "), Some("Lc0 v0.31"));
        assert_eq!(parse_id_name("id name"), None, "bare tag carries no name");
        assert_eq!(parse_id_name("id author T. Romstad et al."), None);
        assert_eq!(parse_id_name("uciok"), None);
        assert_eq!(parse_id_name(""), None);
    }

    /// `identify` against a fake shell "engine": the handshake captures
    /// `id name` and completes without ever starting a search.
    #[tokio::test]
    async fn identify_reads_the_handshake_of_a_scripted_engine() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-engine.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nread _\necho 'id name FakeFish 1.0'\necho 'id author nobody'\necho uciok\nread _\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let id = identify(&script).await.unwrap();
        assert_eq!(id.name.as_deref(), Some("FakeFish 1.0"));
        assert!(id.path.ends_with("fake-engine.sh"));
    }

    /// A binary that is not a UCI engine fails the handshake with an
    /// honest message instead of hanging (bounded by the timeout; `true`
    /// exits immediately, so this returns fast).
    #[tokio::test]
    async fn identify_rejects_a_non_engine_binary() {
        let err = identify(Path::new("/usr/bin/true")).await.unwrap_err();
        assert!(err.contains("not a UCI engine"), "{err}");
    }
}
