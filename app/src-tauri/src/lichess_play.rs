//! Play on Lichess from inside Kibitz via the lichess Board API (run 10,
//! maintainer-endorsed): seek → play → the finished game auto-imports and
//! feeds the existing profile/tactics machinery.
//!
//! Design posture:
//!
//! - **The token is a secret.** The user's personal access token (scope
//!   `board:play`) lives in its own file in the app config dir with 0o600
//!   permissions on unix — never in the database, never logged, never sent
//!   to the frontend in full (status exposes only the username and
//!   "ends in …XXXX").
//! - **Streams get their own threads.** The account event stream
//!   (`/api/stream/event`) and each board game stream
//!   (`/api/board/game/stream/{id}`) are NDJSON long-polls; each runs on a
//!   dedicated thread, reconnects on drops, and pushes state to the
//!   frontend as Tauri events. The strictly-serial [`crate::netops`]
//!   worker stays reserved for bulk ingestion — EXCEPT the finished-game
//!   import, which is enqueued there precisely so it reuses the exact
//!   provenance-recording import path and count-refresh hooks the account
//!   sync already has.
//! - **Move POSTs are direct.** Latency matters in a live game; a move
//!   must never queue behind a TWIC download.
//! - **Fair play is structural (lichess ToS).** Nothing in this module
//!   touches the engine, and the Play screen mounts no analysis surface.
//!   A finished game is imported with provenance and NO analysis jobs are
//!   enqueued (CLAUDE.md #6) — the user can Annotate/Re-analyze explicitly
//!   afterwards like any other game.
//! - **Time controls are honest.** The Board API allows third-party
//!   clients rapid, classical and correspondence only; [`realtime_speed`]
//!   rejects anything faster and the UI says so.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use kibitz_db::import::{import_pgn, ImportStats, SourceInfo, SourceKind};

use crate::browse::DbState;
use crate::netops::{self, NetProgress, NetWorker};

pub(crate) const STANDARD_START_FEN: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// License string recorded in `sources` for imported played games.
pub const PLAY_LICENSE: &str =
    "Lichess game export — the user's own played game (lichess.org API terms of service)";

// ---------------------------------------------------------------------------
// Token storage (config dir, 0o600, never in the db, never echoed in full)
// ---------------------------------------------------------------------------

const TOKEN_FILE: &str = "lichess_token.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenFile {
    token: String,
    /// Lichess username the token authenticated as (fetched at set time).
    /// Not secret; stored so streams and imports know "me" offline.
    username: String,
}

fn token_file_path(dir: &Path) -> PathBuf {
    dir.join(TOKEN_FILE)
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("no app config dir: {e}"))
}

/// Write the token file, creating it 0o600 on unix (owner read/write
/// only). An existing file is truncated and its permissions re-tightened.
fn write_token_file(dir: &Path, tf: &TokenFile) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("config dir: {e}"))?;
    let path = token_file_path(dir);
    let json = serde_json::to_string_pretty(tf).map_err(|e| e.to_string())?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    let mut f = opts.open(&path).map_err(|e| format!("token file: {e}"))?;
    f.write_all(json.as_bytes())
        .map_err(|e| format!("token file: {e}"))?;
    // mode() only applies at creation; re-tighten pre-existing files too.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("token file permissions: {e}"))?;
    }
    Ok(())
}

/// The stored token, if any (missing/corrupt file → None).
fn read_token_file(dir: &Path) -> Option<TokenFile> {
    let bytes = std::fs::read(token_file_path(dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn remove_token_file(dir: &Path) -> Result<(), String> {
    let path = token_file_path(dir);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("removing token file: {e}")),
    }
}

/// Last four characters of the token — the only part ever shown again.
pub(crate) fn token_tail(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let n = chars.len().saturating_sub(4);
    chars[n..].iter().collect()
}

/// What the frontend may know about the stored token. NEVER the token.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LichessTokenStatus {
    pub configured: bool,
    pub username: Option<String>,
    /// Last 4 characters, for "configured · ends in …XXXX".
    pub token_tail: Option<String>,
}

fn status_of(tf: Option<&TokenFile>) -> LichessTokenStatus {
    match tf {
        Some(tf) => LichessTokenStatus {
            configured: true,
            username: Some(tf.username.clone()),
            token_tail: Some(token_tail(&tf.token)),
        },
        None => LichessTokenStatus {
            configured: false,
            username: None,
            token_tail: None,
        },
    }
}

/// Store a lichess personal access token (scope `board:play`), validating
/// it against `/api/account` first — an explicit user action, so the one
/// network request is user-initiated. On failure nothing is stored.
#[tauri::command]
pub async fn lichess_token_set(
    app: AppHandle,
    token: String,
) -> Result<LichessTokenStatus, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("paste a token first".to_string());
    }
    let username = fetch_username(&UreqHttp::new(), &token)?;
    let dir = config_dir(&app)?;
    let tf = TokenFile { token, username };
    write_token_file(&dir, &tf)?;
    Ok(status_of(Some(&tf)))
}

/// Remove the stored token and stop all play streams.
#[tauri::command]
pub async fn lichess_token_clear(
    app: AppHandle,
    play: State<'_, PlayState>,
) -> Result<LichessTokenStatus, String> {
    play.stop.store(true, Ordering::SeqCst);
    play.seek_cancel.store(true, Ordering::SeqCst);
    remove_token_file(&config_dir(&app)?)?;
    Ok(status_of(None))
}

/// Token status for the Settings row. Reads only the local file.
#[tauri::command]
pub async fn lichess_token_status(app: AppHandle) -> Result<LichessTokenStatus, String> {
    let dir = config_dir(&app)?;
    Ok(status_of(read_token_file(&dir).as_ref()))
}

// ---------------------------------------------------------------------------
// HTTP (blocking ureq, same stack as kibitz-db/net; injectable for tests)
// ---------------------------------------------------------------------------

/// Minimal injectable HTTP abstraction for the Board API. Production is
/// [`UreqHttp`]; tests script responses so the suite stays fully offline.
pub(crate) trait PlayHttp: Send + Sync {
    /// GET returning a streaming body reader (NDJSON long-polls included).
    fn get_stream(
        &self,
        url: &str,
        token: &str,
        accept: Option<&str>,
    ) -> Result<Box<dyn Read + Send>, String>;

    /// Form POST returning a streaming body reader (the realtime seek
    /// keeps its connection open; dropping the reader cancels the seek).
    fn post_stream(
        &self,
        url: &str,
        token: &str,
        form: &[(&str, &str)],
    ) -> Result<Box<dyn Read + Send>, String>;
}

/// GET `url` and read the whole body to a string.
fn get_string(http: &dyn PlayHttp, url: &str, token: &str) -> Result<String, String> {
    let mut body = String::new();
    http.get_stream(url, token, None)?
        .read_to_string(&mut body)
        .map_err(|e| format!("reading {url}: {e}"))?;
    Ok(body)
}

/// POST `form` to `url` and read the whole (small) response body.
fn post_string(
    http: &dyn PlayHttp,
    url: &str,
    token: &str,
    form: &[(&str, &str)],
) -> Result<String, String> {
    let mut body = String::new();
    http.post_stream(url, token, form)?
        .read_to_string(&mut body)
        .map_err(|e| format!("reading {url}: {e}"))?;
    Ok(body)
}

/// Production [`PlayHttp`]: blocking `ureq` with the descriptive
/// [`kibitz_db::net::user_agent`]. `read_timeout` is used only by the
/// seek thread so a blocking read can periodically check its cancel flag.
pub(crate) struct UreqHttp {
    read_timeout: Option<Duration>,
}

impl UreqHttp {
    pub(crate) fn new() -> Self {
        Self { read_timeout: None }
    }

    fn with_read_timeout(t: Duration) -> Self {
        Self {
            read_timeout: Some(t),
        }
    }

    fn agent(&self) -> ureq::Agent {
        let mut b = ureq::AgentBuilder::new();
        if let Some(t) = self.read_timeout {
            b = b.timeout_read(t);
        }
        b.build()
    }
}

/// Reduce a ureq error to a message that can be shown and logged safely
/// (the token never appears in it). Lichess error bodies are JSON
/// `{"error": "..."}`; surface that text verbatim, truncated.
fn ureq_err(e: ureq::Error, url: &str) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_string))
                .unwrap_or(body);
            let detail: String = detail.trim().chars().take(200).collect();
            if detail.is_empty() {
                format!("lichess HTTP {code} for {url}")
            } else {
                format!("lichess HTTP {code}: {detail}")
            }
        }
        e => format!("network error for {url}: {e}"),
    }
}

impl PlayHttp for UreqHttp {
    fn get_stream(
        &self,
        url: &str,
        token: &str,
        accept: Option<&str>,
    ) -> Result<Box<dyn Read + Send>, String> {
        let mut req = self
            .agent()
            .get(url)
            .set("User-Agent", kibitz_db::net::user_agent())
            .set("Authorization", &format!("Bearer {token}"));
        if let Some(a) = accept {
            req = req.set("Accept", a);
        }
        match req.call() {
            Ok(resp) => Ok(Box::new(resp.into_reader())),
            Err(e) => Err(ureq_err(e, url)),
        }
    }

    fn post_stream(
        &self,
        url: &str,
        token: &str,
        form: &[(&str, &str)],
    ) -> Result<Box<dyn Read + Send>, String> {
        let req = self
            .agent()
            .post(url)
            .set("User-Agent", kibitz_db::net::user_agent())
            .set("Authorization", &format!("Bearer {token}"));
        let result = if form.is_empty() {
            req.send_string("")
        } else {
            req.send_form(form)
        };
        match result {
            Ok(resp) => Ok(Box::new(resp.into_reader())),
            Err(e) => Err(ureq_err(e, url)),
        }
    }
}

// ---------------------------------------------------------------------------
// Pure protocol helpers (URLs, forms, speed policy) — unit-tested offline
// ---------------------------------------------------------------------------

pub(crate) fn event_stream_url() -> String {
    "https://lichess.org/api/stream/event".to_string()
}

pub(crate) fn game_stream_url(game_id: &str) -> String {
    format!("https://lichess.org/api/board/game/stream/{game_id}")
}

pub(crate) fn move_url(game_id: &str, uci: &str) -> String {
    format!("https://lichess.org/api/board/game/{game_id}/move/{uci}")
}

/// `action` is one of `resign`, `abort`, `draw/yes`, `draw/no`.
pub(crate) fn board_action_url(game_id: &str, action: &str) -> String {
    format!("https://lichess.org/api/board/game/{game_id}/{action}")
}

pub(crate) fn seek_url() -> String {
    "https://lichess.org/api/board/seek".to_string()
}

pub(crate) fn now_playing_url() -> String {
    "https://lichess.org/api/account/playing?nb=50".to_string()
}

/// PGN export of one finished game (clocks/evals off: the import wants
/// clean movetext, and evals would be someone else's analysis anyway).
pub(crate) fn export_url(game_id: &str) -> String {
    format!("https://lichess.org/game/export/{game_id}?evals=false&clocks=false&literate=false")
}

/// Speed class of a realtime seek per lichess's estimate
/// (`minutes*60 + 40*increment` seconds), or None when the Board API
/// forbids it for third-party clients (bullet/blitz — under 8 minutes
/// estimated). This is the single policy point the seek command enforces.
pub(crate) fn realtime_speed(minutes: u32, increment: u32) -> Option<&'static str> {
    let est = u64::from(minutes) * 60 + 40 * u64::from(increment);
    if est < 480 {
        None
    } else if est < 1500 {
        Some("rapid")
    } else {
        Some("classical")
    }
}

/// Form body of a realtime seek (`POST /api/board/seek`).
pub(crate) fn seek_form(
    rated: bool,
    minutes: u32,
    increment: u32,
    color: &str,
) -> Vec<(String, String)> {
    vec![
        ("rated".into(), rated.to_string()),
        ("time".into(), minutes.to_string()),
        ("increment".into(), increment.to_string()),
        ("color".into(), color.to_string()),
    ]
}

/// Form body of a correspondence seek (days per move).
pub(crate) fn corr_seek_form(rated: bool, days: u32, color: &str) -> Vec<(String, String)> {
    vec![
        ("rated".into(), rated.to_string()),
        ("days".into(), days.to_string()),
        ("color".into(), color.to_string()),
    ]
}

/// Fetch the token's account username (`GET /api/account`).
fn fetch_username(http: &dyn PlayHttp, token: &str) -> Result<String, String> {
    let body = get_string(http, "https://lichess.org/api/account", token)?;
    let v: Value =
        serde_json::from_str(&body).map_err(|e| format!("unexpected /api/account body: {e}"))?;
    v["username"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "no username in /api/account response".to_string())
}

// ---------------------------------------------------------------------------
// Game-state machine (pure; the tested heart of the play loop)
// ---------------------------------------------------------------------------

/// Everything the Play screen knows about one game, built exclusively
/// from board-stream messages. Serialized to the frontend on every
/// change (`lichess-play-game` event).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub game_id: String,
    /// "white" | "black"; None when the token's account is not a player
    /// (should not happen on a board stream, but never guessed).
    pub my_color: Option<String>,
    pub white: String,
    pub black: String,
    pub white_rating: Option<u64>,
    pub black_rating: Option<u64>,
    /// "rapid" | "classical" | "correspondence" (as lichess reports it).
    pub speed: String,
    pub rated: bool,
    pub initial_fen: String,
    /// UCI moves from the initial position, in order.
    pub moves: Vec<String>,
    /// Lichess game status: "created" | "started" | "mate" | "resign" |
    /// "draw" | "outoftime" | "aborted" | "stalemate" | …
    pub status: String,
    /// "white" | "black" when the game has a winner.
    pub winner: Option<String>,
    pub wtime_ms: u64,
    pub btime_ms: u64,
    pub winc_ms: u64,
    pub binc_ms: u64,
    /// Pending draw offers, per side.
    pub wdraw: bool,
    pub bdraw: bool,
}

impl GameSnapshot {
    /// Side to move, from the move count (moves are always from the
    /// game's first ply on a board stream).
    pub fn turn(&self) -> &'static str {
        if self.moves.len().is_multiple_of(2) {
            "white"
        } else {
            "black"
        }
    }

    pub fn is_terminal(&self) -> bool {
        terminal_status(&self.status)
    }
}

/// A status other than created/started ends the game (mate, resign,
/// draw, outoftime, aborted, stalemate, nostart, …).
pub(crate) fn terminal_status(status: &str) -> bool {
    !matches!(status, "" | "created" | "started")
}

fn player_name(p: &Value) -> String {
    if let Some(n) = p["name"].as_str() {
        n.to_string()
    } else if let Some(n) = p["id"].as_str() {
        n.to_string()
    } else if let Some(l) = p["aiLevel"].as_u64() {
        format!("Stockfish level {l}")
    } else {
        "?".to_string()
    }
}

fn apply_state(snap: &mut GameSnapshot, s: &Value) {
    if let Some(m) = s["moves"].as_str() {
        snap.moves = m.split_whitespace().map(str::to_string).collect();
    }
    if let Some(t) = s["wtime"].as_u64() {
        snap.wtime_ms = t;
    }
    if let Some(t) = s["btime"].as_u64() {
        snap.btime_ms = t;
    }
    if let Some(t) = s["winc"].as_u64() {
        snap.winc_ms = t;
    }
    if let Some(t) = s["binc"].as_u64() {
        snap.binc_ms = t;
    }
    if let Some(st) = s["status"].as_str() {
        snap.status = st.to_string();
    }
    if let Some(w) = s["winner"].as_str() {
        snap.winner = Some(w.to_string());
    }
    if let Some(d) = s["wdraw"].as_bool() {
        snap.wdraw = d;
    }
    if let Some(d) = s["bdraw"].as_bool() {
        snap.bdraw = d;
    }
}

/// Apply one NDJSON line from a board game stream to the snapshot.
/// Returns true when the snapshot changed (i.e. the line was a
/// `gameFull` or `gameState`; chat and presence lines are ignored).
pub(crate) fn apply_stream_line(snap: &mut GameSnapshot, line: &str, my_username: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    match v["type"].as_str() {
        Some("gameFull") => {
            if let Some(id) = v["id"].as_str() {
                snap.game_id = id.to_string();
            }
            snap.rated = v["rated"].as_bool().unwrap_or(false);
            if let Some(s) = v["speed"].as_str() {
                snap.speed = s.to_string();
            }
            snap.initial_fen = match v["initialFen"].as_str() {
                None | Some("startpos") => STANDARD_START_FEN.to_string(),
                Some(f) => f.to_string(),
            };
            snap.white = player_name(&v["white"]);
            snap.black = player_name(&v["black"]);
            snap.white_rating = v["white"]["rating"].as_u64();
            snap.black_rating = v["black"]["rating"].as_u64();
            snap.my_color = if snap.white.eq_ignore_ascii_case(my_username) {
                Some("white".to_string())
            } else if snap.black.eq_ignore_ascii_case(my_username) {
                Some("black".to_string())
            } else {
                None
            };
            apply_state(snap, &v["state"]);
            true
        }
        Some("gameState") => {
            apply_state(snap, &v);
            true
        }
        _ => false,
    }
}

/// One account-event-stream line reduced to what the Play screen needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayEvent {
    /// "gameStart" | "gameFinish" | "error".
    pub kind: String,
    /// Empty for stream-level errors.
    pub game_id: String,
    /// Opponent username (game events) or the error message.
    pub detail: Option<String>,
}

/// Parse one event-stream NDJSON line; None for keep-alives, challenges
/// and anything else the Play screen does not consume.
pub(crate) fn parse_event_line(line: &str) -> Option<PlayEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let kind = v["type"].as_str()?;
    if kind != "gameStart" && kind != "gameFinish" {
        return None;
    }
    let g = &v["game"];
    let id = g["gameId"].as_str().or_else(|| g["id"].as_str())?;
    Some(PlayEvent {
        kind: kind.to_string(),
        game_id: id.to_string(),
        detail: g["opponent"]["username"].as_str().map(str::to_string),
    })
}

/// One row of `/api/account/playing` — the relaunch/rejoin list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub game_id: String,
    /// "white" | "black" — the account's color.
    pub color: String,
    pub opponent: String,
    pub is_my_turn: bool,
    pub speed: String,
    pub last_move: String,
    pub seconds_left: Option<u64>,
}

pub(crate) fn parse_now_playing(body: &str) -> Result<Vec<NowPlaying>, String> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| format!("unexpected nowPlaying body: {e}"))?;
    let Some(rows) = v["nowPlaying"].as_array() else {
        return Err("no nowPlaying array in response".to_string());
    };
    Ok(rows
        .iter()
        .filter_map(|g| {
            Some(NowPlaying {
                game_id: g["gameId"].as_str()?.to_string(),
                color: g["color"].as_str().unwrap_or("white").to_string(),
                opponent: g["opponent"]["username"]
                    .as_str()
                    .unwrap_or("?")
                    .to_string(),
                is_my_turn: g["isMyTurn"].as_bool().unwrap_or(false),
                speed: g["speed"].as_str().unwrap_or("").to_string(),
                last_move: g["lastMove"].as_str().unwrap_or("").to_string(),
                seconds_left: g["secondsLeft"].as_u64(),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Managed state + stream threads
// ---------------------------------------------------------------------------

/// Play-session state. Threads are per-launch; the durable state (token,
/// game record) lives in the token file and the database respectively.
#[derive(Default)]
pub struct PlayState {
    /// True while the account event-stream thread is alive.
    event_running: Arc<AtomicBool>,
    /// Set on token clear: every stream thread exits at its next check.
    stop: Arc<AtomicBool>,
    /// Game ids with a live board-stream thread.
    streams: Arc<Mutex<HashSet<String>>>,
    /// Latest snapshot per game (so a rejoining frontend gets state
    /// immediately instead of waiting for the next server message).
    snapshots: Arc<Mutex<std::collections::HashMap<String, GameSnapshot>>>,
    /// Game ids already handed to the import worker this launch (the
    /// importer's duplicate detection is the durable guard).
    imported: Arc<Mutex<HashSet<String>>>,
    seek_active: Arc<AtomicBool>,
    seek_cancel: Arc<AtomicBool>,
}

fn require_token(app: &AppHandle) -> Result<TokenFile, String> {
    read_token_file(&config_dir(app)?)
        .ok_or_else(|| "no lichess token configured (Settings → Lichess play)".to_string())
}

fn emit_event(app: &AppHandle, ev: &PlayEvent) {
    let _ = app.emit("lichess-play-event", ev);
}

/// Start the account event stream (idempotent). Returns false when it was
/// already running. The thread reconnects on drops (5 s backoff) and
/// exits on auth failures or token clear.
#[tauri::command]
pub async fn lichess_play_start(
    app: AppHandle,
    play: State<'_, PlayState>,
) -> Result<bool, String> {
    let tf = require_token(&app)?;
    if play.event_running.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    play.stop.store(false, Ordering::SeqCst);
    let running = Arc::clone(&play.event_running);
    let stop = Arc::clone(&play.stop);
    std::thread::spawn(move || {
        let http = UreqHttp::new();
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match http.get_stream(&event_stream_url(), &tf.token, None) {
                Ok(body) => {
                    let reader = BufReader::new(body);
                    for line in reader.lines() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(line) = line else { break }; // dropped — reconnect
                        if line.trim().is_empty() {
                            continue; // NDJSON keep-alive
                        }
                        if let Some(ev) = parse_event_line(&line) {
                            let finished = ev.kind == "gameFinish";
                            let game_id = ev.game_id.clone();
                            emit_event(&app, &ev);
                            if finished {
                                import_finished_game(&app, &game_id);
                            }
                        }
                    }
                }
                Err(e) => {
                    emit_event(
                        &app,
                        &PlayEvent {
                            kind: "error".to_string(),
                            game_id: String::new(),
                            detail: Some(e.clone()),
                        },
                    );
                    if e.contains("HTTP 401") || e.contains("HTTP 403") {
                        break; // bad token; reconnecting would hammer lichess
                    }
                }
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
        running.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

/// Join (or rejoin) a game: ensures a board-stream thread for `game_id`
/// and returns the latest known snapshot immediately, if any. Further
/// state arrives as `lichess-play-game` events.
#[tauri::command]
pub async fn lichess_play_join(
    app: AppHandle,
    play: State<'_, PlayState>,
    game_id: String,
) -> Result<Option<GameSnapshot>, String> {
    let tf = require_token(&app)?;
    let known = play
        .snapshots
        .lock()
        .map_err(|_| "play state poisoned".to_string())?
        .get(&game_id)
        .cloned();
    {
        let mut streams = play
            .streams
            .lock()
            .map_err(|_| "play state poisoned".to_string())?;
        if !streams.insert(game_id.clone()) {
            return Ok(known); // already streaming
        }
    }
    let stop = Arc::clone(&play.stop);
    let streams = Arc::clone(&play.streams);
    let snapshots = Arc::clone(&play.snapshots);
    std::thread::spawn(move || {
        let http = UreqHttp::new();
        let mut snap = GameSnapshot {
            game_id: game_id.clone(),
            ..Default::default()
        };
        'outer: loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match http.get_stream(&game_stream_url(&game_id), &tf.token, None) {
                Ok(body) => {
                    let reader = BufReader::new(body);
                    for line in reader.lines() {
                        if stop.load(Ordering::SeqCst) {
                            break 'outer;
                        }
                        let Ok(line) = line else { break }; // dropped — reconnect
                        if line.trim().is_empty() {
                            continue;
                        }
                        if apply_stream_line(&mut snap, &line, &tf.username) {
                            if let Ok(mut map) = snapshots.lock() {
                                map.insert(game_id.clone(), snap.clone());
                            }
                            let _ = app.emit("lichess-play-game", snap.clone());
                            if snap.is_terminal() {
                                import_finished_game(&app, &game_id);
                                break 'outer;
                            }
                        }
                    }
                }
                Err(e) => {
                    emit_event(
                        &app,
                        &PlayEvent {
                            kind: "error".to_string(),
                            game_id: game_id.clone(),
                            detail: Some(e.clone()),
                        },
                    );
                    if e.contains("HTTP 4") {
                        break; // auth/not-found: reconnecting cannot help
                    }
                }
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_secs(3));
        }
        if let Ok(mut s) = streams.lock() {
            s.remove(&game_id);
        }
    });
    Ok(known)
}

/// Play a move (UCI, e.g. "e2e4" or "e7e8q"). Direct POST — never queued.
#[tauri::command]
pub async fn lichess_play_move(app: AppHandle, game_id: String, uci: String) -> Result<(), String> {
    let tf = require_token(&app)?;
    post_string(&UreqHttp::new(), &move_url(&game_id, &uci), &tf.token, &[]).map(|_| ())
}

/// Resign the game.
#[tauri::command]
pub async fn lichess_play_resign(app: AppHandle, game_id: String) -> Result<(), String> {
    let tf = require_token(&app)?;
    post_string(
        &UreqHttp::new(),
        &board_action_url(&game_id, "resign"),
        &tf.token,
        &[],
    )
    .map(|_| ())
}

/// Abort the game (lichess allows this before both sides have moved).
#[tauri::command]
pub async fn lichess_play_abort(app: AppHandle, game_id: String) -> Result<(), String> {
    let tf = require_token(&app)?;
    post_string(
        &UreqHttp::new(),
        &board_action_url(&game_id, "abort"),
        &tf.token,
        &[],
    )
    .map(|_| ())
}

/// Offer/accept a draw (`accept: true`) or decline one (`false`).
#[tauri::command]
pub async fn lichess_play_draw(
    app: AppHandle,
    game_id: String,
    accept: bool,
) -> Result<(), String> {
    let tf = require_token(&app)?;
    let action = if accept { "draw/yes" } else { "draw/no" };
    post_string(
        &UreqHttp::new(),
        &board_action_url(&game_id, action),
        &tf.token,
        &[],
    )
    .map(|_| ())
}

/// Ongoing games for the rejoin list (`/api/account/playing`).
#[tauri::command]
pub async fn lichess_now_playing(app: AppHandle) -> Result<Vec<NowPlaying>, String> {
    let tf = require_token(&app)?;
    let body = get_string(&UreqHttp::new(), &now_playing_url(), &tf.token)?;
    parse_now_playing(&body)
}

/// Seek a game. Realtime seeks (rapid/classical ONLY — the Board API
/// forbids bullet/blitz for third-party clients) hold their connection
/// open on a dedicated thread until matched or cancelled; correspondence
/// seeks (`days`) return immediately. `color` is "random"|"white"|"black".
#[tauri::command]
pub async fn lichess_play_seek(
    app: AppHandle,
    play: State<'_, PlayState>,
    minutes: Option<u32>,
    increment: Option<u32>,
    days: Option<u32>,
    rated: bool,
    color: Option<String>,
) -> Result<(), String> {
    let tf = require_token(&app)?;
    let color = color.unwrap_or_else(|| "random".to_string());
    if let Some(days) = days {
        if !matches!(days, 1 | 2 | 3 | 5 | 7 | 10 | 14) {
            return Err("correspondence days must be one of 1, 2, 3, 5, 7, 10, 14".to_string());
        }
        let form = corr_seek_form(rated, days, &color);
        let form_ref: Vec<(&str, &str)> =
            form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        // Correspondence seeks are created server-side and return promptly.
        post_string(&UreqHttp::new(), &seek_url(), &tf.token, &form_ref).map(|_| ())?;
        return Ok(());
    }
    let (minutes, increment) = (minutes.unwrap_or(0), increment.unwrap_or(0));
    if realtime_speed(minutes, increment).is_none() {
        return Err(
            "the lichess Board API allows rapid, classical and correspondence only — \
             no bullet or blitz (estimated duration must be at least 8 minutes)"
                .to_string(),
        );
    }
    if play.seek_active.swap(true, Ordering::SeqCst) {
        return Err("a seek is already running".to_string());
    }
    play.seek_cancel.store(false, Ordering::SeqCst);
    let active = Arc::clone(&play.seek_active);
    let cancel = Arc::clone(&play.seek_cancel);
    std::thread::spawn(move || {
        let _ = app.emit(
            "lichess-play-seek",
            serde_json::json!({ "active": true, "error": null }),
        );
        // Short read timeout so the blocking read can poll the cancel
        // flag; dropping the reader closes the connection, which is how
        // a lichess realtime seek is cancelled.
        let http = UreqHttp::with_read_timeout(Duration::from_secs(2));
        let form = seek_form(rated, minutes, increment, &color);
        let form_ref: Vec<(&str, &str)> =
            form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let error = match http.post_stream(&seek_url(), &tf.token, &form_ref) {
            Ok(mut body) => {
                let mut buf = [0u8; 256];
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        break None; // drop closes the connection → seek cancelled
                    }
                    match body.read(&mut buf) {
                        Ok(0) => break None, // matched (gameStart arrives on the event stream)
                        Ok(_) => {}
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                            ) => {}
                        Err(e) => break Some(format!("seek stream: {e}")),
                    }
                }
            }
            Err(e) => Some(e),
        };
        active.store(false, Ordering::SeqCst);
        let _ = app.emit(
            "lichess-play-seek",
            serde_json::json!({ "active": false, "error": error }),
        );
    });
    Ok(())
}

/// Cancel a running realtime seek. Returns false when none was running.
#[tauri::command]
pub async fn lichess_seek_cancel(play: State<'_, PlayState>) -> Result<bool, String> {
    let was = play.seek_active.load(Ordering::SeqCst);
    if was {
        play.seek_cancel.store(true, Ordering::SeqCst);
    }
    Ok(was)
}

// ---------------------------------------------------------------------------
// Finished-game import (the existing import machinery, with provenance)
// ---------------------------------------------------------------------------

/// Import one finished game through the EXISTING import path, with
/// provenance. Pure with respect to I/O: the PGN bytes are already
/// fetched; unit-tested offline against a fixture PGN.
pub(crate) fn import_played_pgn(
    conn: &rusqlite::Connection,
    pgn: &[u8],
    username: &str,
    game_id: &str,
    origin_url: &str,
) -> Result<ImportStats, String> {
    let source = SourceInfo {
        name: format!("lichess play: {username} {game_id}"),
        origin: origin_url.to_string(),
        license: PLAY_LICENSE.to_string(),
        kind: SourceKind::Online,
    };
    import_pgn(conn, &source, std::io::Cursor::new(pgn.to_vec()))
        .map_err(|e| format!("importing lichess game {game_id}: {e:#}"))
}

/// Enqueue the finished game's export+import on the serial network worker
/// — the same worker (and therefore the same frontend refresh hooks) the
/// account sync uses. NO analysis job is enqueued (CLAUDE.md #6): the
/// user can Annotate/Re-analyze the imported game explicitly afterwards.
fn import_finished_game(app: &AppHandle, game_id: &str) {
    let play: State<'_, PlayState> = app.state();
    {
        let Ok(mut imported) = play.imported.lock() else {
            return;
        };
        if !imported.insert(game_id.to_string()) {
            return; // both the event stream and the game stream saw the finish
        }
    }
    let Some(tf) = config_dir(app).ok().and_then(|d| read_token_file(&d)) else {
        return;
    };
    let db_state: State<'_, DbState> = app.state();
    let Ok(db_path) = netops::open_db_path(&db_state) else {
        return; // no database open — nothing to import into
    };
    let worker: State<'_, NetWorker> = app.state();
    let game_id = game_id.to_string();
    let initial = NetProgress {
        kind: "lichess-play".to_string(),
        label: format!("Lichess play: {game_id}"),
        done: 0,
        total: 0,
        detail: "fetching the finished game…".to_string(),
        active: true,
        queued: Vec::new(),
        error: None,
    };
    let app_handle = app.clone();
    let _ = netops::spawn_net_worker(&worker, initial, move |_stop, progress| {
        let conn = netops::worker_conn(&db_path)?;
        let url = export_url(&game_id);
        let mut pgn = Vec::new();
        UreqHttp::new()
            .get_stream(&url, &tf.token, Some("application/x-chess-pgn"))?
            .read_to_end(&mut pgn)
            .map_err(|e| format!("reading game export: {e}"))?;
        let stats = import_played_pgn(&conn, &pgn, &tf.username, &game_id, &url)?;
        if let Ok(mut guard) = progress.lock() {
            if let Some(p) = guard.as_mut() {
                p.detail = format!(
                    "game {game_id}: {} imported · {} duplicate{}",
                    stats.games_imported,
                    stats.duplicates_skipped,
                    if stats.duplicates_skipped == 1 {
                        ""
                    } else {
                        "s"
                    }
                );
            }
        }
        let _ = app_handle.emit(
            "lichess-play-event",
            PlayEvent {
                kind: "imported".to_string(),
                game_id: game_id.clone(),
                detail: None,
            },
        );
        Ok(())
    });
}

// ---------------------------------------------------------------------------
// Tests (fully offline; fixtures follow the lichess API documentation)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- token storage ----

    #[test]
    fn token_file_round_trips_and_is_owner_only_on_unix() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("cfg");
        let tf = TokenFile {
            token: "lip_abcdef123456".to_string(),
            username: "SomeUser".to_string(),
        };
        write_token_file(&nested, &tf).unwrap();
        let read = read_token_file(&nested).unwrap();
        assert_eq!(read.token, "lip_abcdef123456");
        assert_eq!(read.username, "SomeUser");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(token_file_path(&nested))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "token file must be owner-only");
        }
        // Clear removes it; clearing twice is fine.
        remove_token_file(&nested).unwrap();
        assert!(read_token_file(&nested).is_none());
        remove_token_file(&nested).unwrap();
    }

    #[test]
    fn token_status_exposes_only_username_and_tail_never_the_token() {
        let tf = TokenFile {
            token: "lip_secretsecretXYZW".to_string(),
            username: "SomeUser".to_string(),
        };
        let s = status_of(Some(&tf));
        assert!(s.configured);
        assert_eq!(s.username.as_deref(), Some("SomeUser"));
        assert_eq!(s.token_tail.as_deref(), Some("XYZW"));
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("secretsecret"),
            "the wire payload must never carry the token: {json}"
        );
        let none = status_of(None);
        assert!(!none.configured);
        assert_eq!(none.token_tail, None);
    }

    #[test]
    fn token_tail_handles_short_tokens() {
        assert_eq!(token_tail("abc"), "abc");
        assert_eq!(token_tail("abcdefgh"), "efgh");
    }

    // ---- speed policy (fair, honest time controls) ----

    #[test]
    fn realtime_speed_rejects_bullet_and_blitz_allows_rapid_and_classical() {
        assert_eq!(realtime_speed(1, 0), None, "bullet");
        assert_eq!(realtime_speed(3, 2), None, "blitz 3+2");
        assert_eq!(realtime_speed(5, 3), None, "blitz 5+3 (7 min estimate)");
        assert_eq!(realtime_speed(8, 0), Some("rapid"));
        assert_eq!(realtime_speed(10, 5), Some("rapid"));
        assert_eq!(
            realtime_speed(5, 10),
            Some("rapid"),
            "estimate crosses 8 min"
        );
        assert_eq!(realtime_speed(25, 0), Some("classical"));
        assert_eq!(realtime_speed(15, 30), Some("classical"));
    }

    #[test]
    fn seek_forms_carry_the_documented_fields() {
        let f = seek_form(true, 10, 5, "white");
        assert_eq!(
            f,
            vec![
                ("rated".to_string(), "true".to_string()),
                ("time".to_string(), "10".to_string()),
                ("increment".to_string(), "5".to_string()),
                ("color".to_string(), "white".to_string()),
            ]
        );
        let c = corr_seek_form(false, 3, "random");
        assert_eq!(
            c,
            vec![
                ("rated".to_string(), "false".to_string()),
                ("days".to_string(), "3".to_string()),
                ("color".to_string(), "random".to_string()),
            ]
        );
    }

    #[test]
    fn urls_are_the_documented_board_api_endpoints() {
        assert_eq!(
            move_url("abc123", "e7e8q"),
            "https://lichess.org/api/board/game/abc123/move/e7e8q"
        );
        assert_eq!(
            game_stream_url("abc123"),
            "https://lichess.org/api/board/game/stream/abc123"
        );
        assert_eq!(
            board_action_url("abc123", "draw/yes"),
            "https://lichess.org/api/board/game/abc123/draw/yes"
        );
        assert_eq!(
            export_url("abc123"),
            "https://lichess.org/game/export/abc123?evals=false&clocks=false&literate=false"
        );
    }

    // ---- game-state machine ----

    /// gameFull fixture shaped per the lichess Board API docs.
    const GAME_FULL: &str = r#"{"type":"gameFull","id":"abc123","rated":true,
        "variant":{"key":"standard"},"speed":"rapid","initialFen":"startpos",
        "white":{"id":"someuser","name":"SomeUser","rating":1500},
        "black":{"id":"opponent","name":"Opponent","rating":1520},
        "state":{"type":"gameState","moves":"e2e4 e7e5","wtime":600000,
        "btime":598000,"winc":5000,"binc":5000,"status":"started"}}"#;

    fn full_snapshot() -> GameSnapshot {
        let mut snap = GameSnapshot::default();
        assert!(apply_stream_line(
            &mut snap,
            &GAME_FULL.replace('\n', " "),
            "someuser"
        ));
        snap
    }

    #[test]
    fn game_full_builds_the_snapshot_and_detects_my_color_case_insensitively() {
        let snap = full_snapshot();
        assert_eq!(snap.game_id, "abc123");
        assert!(snap.rated);
        assert_eq!(snap.speed, "rapid");
        assert_eq!(snap.initial_fen, STANDARD_START_FEN, "startpos normalized");
        assert_eq!(snap.white, "SomeUser");
        assert_eq!(snap.black, "Opponent");
        assert_eq!(snap.white_rating, Some(1500));
        assert_eq!(snap.my_color.as_deref(), Some("white"), "case-insensitive");
        assert_eq!(snap.moves, vec!["e2e4", "e7e5"]);
        assert_eq!(snap.wtime_ms, 600000);
        assert_eq!(snap.btime_ms, 598000);
        assert_eq!(snap.status, "started");
        assert_eq!(snap.turn(), "white", "two moves played — white to move");
        assert!(!snap.is_terminal());
    }

    #[test]
    fn game_state_advances_moves_clocks_and_draw_offers() {
        let mut snap = full_snapshot();
        let state = r#"{"type":"gameState","moves":"e2e4 e7e5 g1f3","wtime":590000,
            "btime":598000,"winc":5000,"binc":5000,"status":"started","bdraw":true}"#
            .replace('\n', " ");
        assert!(apply_stream_line(&mut snap, &state, "someuser"));
        assert_eq!(snap.moves.len(), 3);
        assert_eq!(snap.turn(), "black");
        assert_eq!(snap.wtime_ms, 590000);
        assert!(snap.bdraw, "black's draw offer is visible");
        assert!(!snap.wdraw);
        // Identity fields survive gameState lines (they carry no players).
        assert_eq!(snap.white, "SomeUser");
        assert_eq!(snap.my_color.as_deref(), Some("white"));
    }

    #[test]
    fn terminal_states_are_recognized_with_winner() {
        let mut snap = full_snapshot();
        let end = r#"{"type":"gameState","moves":"e2e4 e7e5 f1c4 b8c6 d1h5 g8f6 h5f7",
            "wtime":570000,"btime":580000,"winc":5000,"binc":5000,
            "status":"mate","winner":"white"}"#
            .replace('\n', " ");
        assert!(apply_stream_line(&mut snap, &end, "someuser"));
        assert!(snap.is_terminal());
        assert_eq!(snap.winner.as_deref(), Some("white"));
        for s in [
            "mate",
            "resign",
            "draw",
            "outoftime",
            "aborted",
            "stalemate",
        ] {
            assert!(terminal_status(s), "{s} ends the game");
        }
        for s in ["created", "started", ""] {
            assert!(!terminal_status(s), "{s} does not end the game");
        }
    }

    #[test]
    fn chat_and_unknown_lines_change_nothing() {
        let mut snap = full_snapshot();
        let before = snap.clone();
        assert!(!apply_stream_line(
            &mut snap,
            r#"{"type":"chatLine","username":"Opponent","text":"gl","room":"player"}"#,
            "someuser"
        ));
        assert!(!apply_stream_line(&mut snap, "not json at all", "someuser"));
        assert_eq!(snap, before);
    }

    #[test]
    fn rejoin_a_game_in_progress_restores_the_full_move_list() {
        // The board stream always opens with gameFull carrying every move
        // so far — this is what makes app-restart rejoin work.
        let mut snap = GameSnapshot::default();
        let mid = GAME_FULL.replace(
            r#""moves":"e2e4 e7e5""#,
            r#""moves":"e2e4 e7e5 g1f3 b8c6 f1b5 a7a6""#,
        );
        assert!(apply_stream_line(
            &mut snap,
            &mid.replace('\n', " "),
            "opponent"
        ));
        assert_eq!(snap.moves.len(), 6);
        assert_eq!(
            snap.my_color.as_deref(),
            Some("black"),
            "token user is black"
        );
        assert_eq!(snap.turn(), "white");
    }

    #[test]
    fn ai_opponents_get_a_readable_name() {
        let mut snap = GameSnapshot::default();
        let vs_ai = GAME_FULL.replace(
            r#"{"id":"opponent","name":"Opponent","rating":1520}"#,
            r#"{"aiLevel":3}"#,
        );
        assert!(apply_stream_line(
            &mut snap,
            &vs_ai.replace('\n', " "),
            "someuser"
        ));
        assert_eq!(snap.black, "Stockfish level 3");
        assert_eq!(snap.black_rating, None);
    }

    // ---- event stream + nowPlaying parsing ----

    #[test]
    fn event_lines_reduce_to_game_start_and_finish() {
        let start = r#"{"type":"gameStart","game":{"gameId":"rCRw1AuO","fullId":"rCRw1AuOvonq",
            "color":"white","speed":"rapid","opponent":{"id":"opp","username":"Opp","rating":1500},
            "isMyTurn":true}}"#
            .replace('\n', " ");
        let ev = parse_event_line(&start).unwrap();
        assert_eq!(ev.kind, "gameStart");
        assert_eq!(ev.game_id, "rCRw1AuO");
        assert_eq!(ev.detail.as_deref(), Some("Opp"));

        let finish =
            r#"{"type":"gameFinish","game":{"id":"rCRw1AuO","status":{"id":30,"name":"mate"}}}"#;
        let ev = parse_event_line(finish).unwrap();
        assert_eq!(ev.kind, "gameFinish");
        assert_eq!(ev.game_id, "rCRw1AuO", "legacy id key accepted");

        assert!(parse_event_line(r#"{"type":"challenge","challenge":{}}"#).is_none());
        assert!(parse_event_line("").is_none());
    }

    #[test]
    fn now_playing_rows_parse_for_the_rejoin_list() {
        let body = r#"{"nowPlaying":[{"gameId":"abc123","fullId":"abc123defg",
            "color":"black","lastMove":"e2e4","speed":"correspondence","isMyTurn":true,
            "secondsLeft":259200,"opponent":{"id":"opp","username":"Opp","rating":1450}}]}"#
            .replace('\n', " ");
        let rows = parse_now_playing(&body).unwrap();
        assert_eq!(rows.len(), 1);
        let g = &rows[0];
        assert_eq!(g.game_id, "abc123");
        assert_eq!(g.color, "black");
        assert_eq!(g.opponent, "Opp");
        assert!(g.is_my_turn);
        assert_eq!(g.speed, "correspondence");
        assert_eq!(g.seconds_left, Some(259200));
        assert!(parse_now_playing("{}").is_err());
    }

    // ---- offline HTTP fixture (kibitz-db FixtureFetcher pattern) ----

    struct FixtureHttp {
        bodies: std::collections::HashMap<String, Vec<u8>>,
        log: Mutex<Vec<String>>,
    }

    impl FixtureHttp {
        fn new() -> Self {
            Self {
                bodies: std::collections::HashMap::new(),
                log: Mutex::new(Vec::new()),
            }
        }
    }

    impl PlayHttp for FixtureHttp {
        fn get_stream(
            &self,
            url: &str,
            _token: &str,
            _accept: Option<&str>,
        ) -> Result<Box<dyn Read + Send>, String> {
            self.log.lock().unwrap().push(format!("GET {url}"));
            match self.bodies.get(url) {
                Some(b) => Ok(Box::new(std::io::Cursor::new(b.clone()))),
                None => Err(format!("lichess HTTP 404 for {url}")),
            }
        }
        fn post_stream(
            &self,
            url: &str,
            _token: &str,
            form: &[(&str, &str)],
        ) -> Result<Box<dyn Read + Send>, String> {
            let body: Vec<String> = form.iter().map(|(k, v)| format!("{k}={v}")).collect();
            self.log
                .lock()
                .unwrap()
                .push(format!("POST {url} [{}]", body.join("&")));
            match self.bodies.get(url) {
                Some(b) => Ok(Box::new(std::io::Cursor::new(b.clone()))),
                None => Err(format!("lichess HTTP 404 for {url}")),
            }
        }
    }

    #[test]
    fn fetch_username_reads_the_account_endpoint() {
        let mut http = FixtureHttp::new();
        http.bodies.insert(
            "https://lichess.org/api/account".to_string(),
            br#"{"id":"someuser","username":"SomeUser"}"#.to_vec(),
        );
        assert_eq!(fetch_username(&http, "tok").unwrap(), "SomeUser");
        // The request itself never embeds the token in the URL.
        assert_eq!(
            http.log.lock().unwrap().as_slice(),
            &["GET https://lichess.org/api/account".to_string()]
        );
    }

    #[test]
    fn move_post_hits_the_documented_url_with_an_empty_form() {
        let mut http = FixtureHttp::new();
        http.bodies.insert(
            "https://lichess.org/api/board/game/abc123/move/e2e4".to_string(),
            br#"{"ok":true}"#.to_vec(),
        );
        post_string(&http, &move_url("abc123", "e2e4"), "tok", &[]).unwrap();
        assert_eq!(
            http.log.lock().unwrap().as_slice(),
            &["POST https://lichess.org/api/board/game/abc123/move/e2e4 []".to_string()]
        );
    }

    // ---- finished-game import (existing machinery, with provenance) ----

    const PLAYED_PGN: &[u8] = br#"[Event "Rated rapid game"]
[Site "https://lichess.org/abc123"]
[White "SomeUser"]
[Black "Opponent"]
[Result "1-0"]
[UTCDate "2026.07.28"]
[UTCTime "12:00:00"]

1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0
"#;

    #[test]
    fn finished_game_imports_once_with_online_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let url = export_url("abc123");
        let stats = import_played_pgn(&conn, PLAYED_PGN, "SomeUser", "abc123", &url).unwrap();
        assert_eq!(stats.games_imported, 1, "failures: {:?}", stats.failures);

        let (name, origin, license, kind): (String, String, String, String) = conn
            .query_row(
                "SELECT name, origin, license, kind FROM sources ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(name, "lichess play: SomeUser abc123");
        assert_eq!(origin, url);
        assert_eq!(license, PLAY_LICENSE);
        assert_eq!(kind, "online", "counts as a personal/online game on Home");

        // A second import of the same game is a duplicate, not a copy.
        let again = import_played_pgn(&conn, PLAYED_PGN, "SomeUser", "abc123", &url).unwrap();
        assert_eq!(again.games_imported, 0);
        assert_eq!(again.duplicates_skipped, 1);
        let games: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(games, 1);
    }

    #[test]
    fn export_fetch_then_import_via_the_fixture_http() {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        let url = export_url("abc123");
        let mut http = FixtureHttp::new();
        http.bodies.insert(url.clone(), PLAYED_PGN.to_vec());

        let mut pgn = Vec::new();
        http.get_stream(&url, "tok", Some("application/x-chess-pgn"))
            .unwrap()
            .read_to_end(&mut pgn)
            .unwrap();
        let stats = import_played_pgn(&conn, &pgn, "SomeUser", "abc123", &url).unwrap();
        assert_eq!(stats.games_imported, 1);
    }
}
