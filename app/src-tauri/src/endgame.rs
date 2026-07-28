//! Endgame trainer IPC commands (ROADMAP Phase 5) over kibitz_db::endgame:
//! curriculum + progress overview, drill sessions (user vs tablebase/
//! heuristic opponent) and attempt recording.
//!
//! Engine-off principle (CLAUDE.md #6): nothing here can spawn an engine.
//! The opponent is Syzygy tablebase probing (kibitz-tb / Fathom, in
//! process) where the piece count is covered, else the deterministic
//! heuristic documented in kibitz_db::endgame. With only the 3-man test
//! set most drills use the heuristic; a 3-4-5 set covers the whole
//! curriculum.
//!
//! The Syzygy directory resolves from KIBITZ_SYZYGY, else by walking up
//! from the cwd looking for `testdata/syzygy` (the dev layout, mirroring
//! the Stockfish and puzzle-CSV resolution conventions). Fathom keeps
//! process-global state, so the Tablebase is initialized once and kept for
//! the app's lifetime.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use kibitz_db::endgame::{self, DrillProgress, DrillSession, Goal, StepReport, Tier};
use kibitz_tb::Tablebase;
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

enum TbSlot {
    Untried,
    Unavailable(String),
    Ready(Box<Tablebase>),
}

struct RunningDrill {
    session: DrillSession,
    started: Instant,
}

struct Inner {
    tb: TbSlot,
    /// User-configured Syzygy directory (Settings' engine manager, run
    /// 10); takes priority over KIBITZ_SYZYGY and the dev-layout walk.
    dir_override: Option<PathBuf>,
    running: Option<RunningDrill>,
}

/// Endgame trainer state: the (at most one) running drill session and the
/// lazily initialized process-wide tablebase handle.
pub struct EndgameState(Mutex<Inner>);

impl Default for EndgameState {
    fn default() -> Self {
        EndgameState(Mutex::new(Inner {
            tb: TbSlot::Untried,
            dir_override: None,
            running: None,
        }))
    }
}

fn resolve_tb_dir(dir_override: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(p) = dir_override {
        // An explicit choice is never silently substituted: if it is
        // gone, resolution fails (with the honest note) rather than
        // falling back behind the user's back.
        return p.is_dir().then(|| p.clone());
    }
    if let Ok(p) = std::env::var("KIBITZ_SYZYGY") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("testdata/syzygy");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

impl Inner {
    /// Initialize the tablebase on first use; remember failures so the
    /// resolution runs once, not per move.
    fn ensure_tb(&mut self) {
        if matches!(self.tb, TbSlot::Untried) {
            self.tb = match resolve_tb_dir(self.dir_override.as_ref()) {
                None => TbSlot::Unavailable(match &self.dir_override {
                    Some(p) => format!("configured directory does not exist: {}", p.display()),
                    None => "no Syzygy directory found (set KIBITZ_SYZYGY or fetch \
                             testdata/syzygy)"
                        .to_string(),
                }),
                Some(dir) => match Tablebase::init(&dir) {
                    Ok(tb) => TbSlot::Ready(Box::new(tb)),
                    Err(e) => TbSlot::Unavailable(format!("{}: {e}", dir.display())),
                },
            };
        }
    }

    fn tb(&mut self) -> Option<&mut Tablebase> {
        match &mut self.tb {
            TbSlot::Ready(tb) => Some(tb.as_mut()),
            _ => None,
        }
    }

    /// TbInfo snapshot of the (ensured) slot — the single source for the
    /// endgame screen, Settings and the engine manager.
    fn tb_info(&self) -> TbInfo {
        match &self.tb {
            TbSlot::Ready(tb) => TbInfo {
                available: true,
                largest: Some(tb.largest()),
                note: format!(
                    "Syzygy tables loaded (up to {} men): optimal replies and instant \
                     blunder detection where the piece count is covered.",
                    tb.largest()
                ),
            },
            TbSlot::Unavailable(why) => TbInfo {
                available: false,
                largest: None,
                note: format!(
                    "No tablebase ({why}); the opponent is a simple heuristic and only \
                     checkmate/stalemate/draw endings are detected."
                ),
            },
            TbSlot::Untried => unreachable!("ensure_tb ran"),
        }
    }
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TbInfo {
    pub available: bool,
    /// Largest piece count the loaded tables cover (e.g. 3 or 5).
    pub largest: Option<u32>,
    /// Human-readable status for the UI.
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrillInfo {
    pub id: String,
    pub tier: String,
    pub title: String,
    pub concept: String,
    pub material: String,
    pub fen: String,
    pub goal: Goal,
    pub instruction: String,
    pub attempts: i64,
    pub solved: i64,
    pub clean_streak: i64,
    pub mastered: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub tiers: Vec<Tier>,
    pub drills: Vec<DrillInfo>,
    pub mastery_streak: i64,
    pub tablebase: TbInfo,
}

/// Curriculum merged with the user's progress, plus tablebase status.
#[tauri::command]
pub async fn endgame_overview(
    db: State<'_, DbState>,
    eg: State<'_, EndgameState>,
) -> Result<Overview, String> {
    let progress = with_conn(&db, |conn| {
        endgame::progress_all(conn).map_err(|e| e.to_string())
    })?;
    let mut inner = eg.0.lock().map_err(|e| e.to_string())?;
    inner.ensure_tb();
    let tablebase = inner.tb_info();
    let c = endgame::curriculum();
    let drills = c
        .drills
        .iter()
        .map(|d| {
            let p = progress.iter().find(|p| p.drill_id == d.id);
            DrillInfo {
                id: d.id.clone(),
                tier: d.tier.clone(),
                title: d.title.clone(),
                concept: d.concept.clone(),
                material: d.material.clone(),
                fen: d.fen.clone(),
                goal: d.goal,
                instruction: d.instruction.clone(),
                attempts: p.map_or(0, |p| p.attempts),
                solved: p.map_or(0, |p| p.solved),
                clean_streak: p.map_or(0, |p| p.clean_streak),
                mastered: p.is_some_and(|p| p.mastered),
            }
        })
        .collect();
    Ok(Overview {
        tiers: c.tiers.clone(),
        drills,
        mastery_streak: endgame::MASTERY_STREAK,
        tablebase,
    })
}

/// Tablebase status alone (no database required) — the engine manager's
/// Syzygy row.
#[tauri::command]
pub async fn tablebase_status(eg: State<'_, EndgameState>) -> Result<TbInfo, String> {
    let mut inner = eg.0.lock().map_err(|e| e.to_string())?;
    inner.ensure_tb();
    Ok(inner.tb_info())
}

/// Set (or with `None` clear) the user-configured Syzygy directory and
/// re-resolve immediately; returns the resulting status. The frontend
/// persists the choice (localStorage) and pushes it at launch. Fathom
/// keeps process-global state, so re-init replaces the loaded set.
#[tauri::command]
pub async fn set_tablebase_dir(
    eg: State<'_, EndgameState>,
    dir: Option<String>,
) -> Result<TbInfo, String> {
    let dir = dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    let mut inner = eg.0.lock().map_err(|e| e.to_string())?;
    inner.dir_override = dir.map(PathBuf::from);
    inner.tb = TbSlot::Untried;
    inner.ensure_tb();
    Ok(inner.tb_info())
}

// ---------------------------------------------------------------------------
// Drill sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedDrill {
    pub drill_id: String,
    pub title: String,
    pub instruction: String,
    pub goal: Goal,
    pub fen: String,
    /// "white" | "black" — the side the user plays (the side to move).
    pub user_side: String,
    /// Whether the opponent's replies will come from the tablebase in the
    /// starting position (captures can bring bigger drills into coverage).
    pub opponent_tablebase: bool,
}

/// Start (or restart) a drill; any running drill is discarded unrecorded —
/// the UI records give-ups explicitly via `endgame_give_up`.
#[tauri::command]
pub async fn endgame_start(
    eg: State<'_, EndgameState>,
    drill_id: String,
) -> Result<StartedDrill, String> {
    let drill = endgame::drill(&drill_id).ok_or_else(|| format!("unknown drill {drill_id:?}"))?;
    let session = DrillSession::new(drill).map_err(|e| e.to_string())?;
    let mut inner = eg.0.lock().map_err(|e| e.to_string())?;
    inner.ensure_tb();
    let started = StartedDrill {
        drill_id: drill.id.clone(),
        title: drill.title.clone(),
        instruction: drill.instruction.clone(),
        goal: drill.goal,
        fen: session.fen(),
        user_side: match session.user_color() {
            cozy_chess::Color::White => "white".to_string(),
            cozy_chess::Color::Black => "black".to_string(),
        },
        opponent_tablebase: session.opponent_would_use_tb(inner.tb().map(|tb| &*tb)),
    };
    inner.running = Some(RunningDrill {
        session,
        started: Instant::now(),
    });
    Ok(started)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResponse {
    #[serde(flatten)]
    pub step: StepReport,
    /// Set when this move ended the drill (the attempt is then recorded).
    pub progress: Option<DrillProgress>,
}

/// Play one user move; the opponent replies within the same call. When the
/// drill ends, the attempt is recorded and the session cleared.
#[tauri::command]
pub async fn endgame_move(
    db: State<'_, DbState>,
    eg: State<'_, EndgameState>,
    uci: String,
) -> Result<MoveResponse, String> {
    let (step, finished) = {
        let mut guard = eg.0.lock().map_err(|e| e.to_string())?;
        let inner = &mut *guard;
        let tb = match &mut inner.tb {
            TbSlot::Ready(tb) => Some(tb.as_mut()),
            _ => None,
        };
        let running = inner.running.as_mut().ok_or("no drill is running")?;
        let step = running
            .session
            .user_move(&uci, tb)
            .map_err(|e| e.to_string())?;
        let finished = step.outcome.as_ref().map(|o| {
            (
                o.solved,
                running.session.drill().id.clone(),
                running.session.user_moves(),
                running.started.elapsed().as_millis() as i64,
                running.session.opponent_kind(),
                running.session.verification_kind(),
            )
        });
        if finished.is_some() {
            inner.running = None;
        }
        (step, finished)
    };
    let progress = match finished {
        Some((solved, drill_id, moves, time_ms, opponent, verification)) => {
            Some(with_conn(&db, |conn| {
                endgame::record_attempt(
                    conn,
                    &drill_id,
                    solved,
                    moves,
                    time_ms,
                    opponent,
                    verification,
                )
                .map_err(|e| e.to_string())
            })?)
        }
        None => None,
    };
    Ok(MoveResponse { step, progress })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GiveUpResponse {
    pub progress: DrillProgress,
}

/// Concede the running drill: recorded as a failed attempt.
#[tauri::command]
pub async fn endgame_give_up(
    db: State<'_, DbState>,
    eg: State<'_, EndgameState>,
) -> Result<GiveUpResponse, String> {
    let (drill_id, moves, time_ms, opponent, verification) = {
        let mut guard = eg.0.lock().map_err(|e| e.to_string())?;
        let running = guard.running.take().ok_or("no drill is running")?;
        let mut session = running.session;
        session.resign();
        (
            session.drill().id.clone(),
            session.user_moves(),
            running.started.elapsed().as_millis() as i64,
            session.opponent_kind(),
            session.verification_kind(),
        )
    };
    let progress = with_conn(&db, |conn| {
        endgame::record_attempt(
            conn,
            &drill_id,
            false,
            moves,
            time_ms,
            opponent,
            verification,
        )
        .map_err(|e| e.to_string())
    })?;
    Ok(GiveUpResponse { progress })
}
