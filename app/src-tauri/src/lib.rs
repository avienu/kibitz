//! Silman Tauri shell.
//!
//! Exposes IPC commands over the UCI manager (src/uci.rs) —
//! `resolve_engine_path`, `analyze_position`, `stop_analysis` — the
//! read-only database browser (src/browse.rs) — `open_database`,
//! `list_games`, `get_game`, `opening_tree`, `find_games_at` — the Phase 2
//! surfaces: opponent prep (src/prep.rs: `matching_players`, `prep_view`),
//! annotation editing (src/tokens.rs: `get_game_tokens`,
//! `update_game_tokens`), and static position explanation (src/explain.rs:
//! `explain_position`).
//! Search progress streams to the frontend as `engine-info` events and the
//! run terminates with a single `engine-done` event.
//!
//! Product principle (CLAUDE.md #6): the engine is spawned lazily on the
//! first user-initiated `analyze_position` and never runs unprompted.
//! Database browsing never touches the engine.

pub mod browse;
pub mod dbops;
pub mod endgame;
pub mod explain;
pub mod home;
pub mod prep;
pub mod tactics;
pub mod tokens;
pub mod train;
pub mod uci;

use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

use uci::{Engine, StopHandle, UciPosition};

/// Shared engine state. `engine` is locked for the duration of a search,
/// serializing searches; `stop` stays accessible so `stop_analysis` can
/// interrupt a search that currently holds the engine lock.
#[derive(Default)]
pub struct EngineState {
    engine: Arc<Mutex<Option<Engine>>>,
    stop: std::sync::Mutex<Option<StopHandle>>,
}

/// Terminal event payload for an analysis run (`engine-done`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DonePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    bestmove: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ponder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Report which engine binary would be used for `user_path` (may be null).
/// Resolution order: user path > SILMAN_STOCKFISH > repo binary > PATH.
#[tauri::command]
fn resolve_engine_path(user_path: Option<String>) -> Result<String, String> {
    uci::resolve_engine_path(user_path.as_deref()).map(|p| p.display().to_string())
}

/// Start `go nodes <nodes>` on `fen`. Returns as soon as the search task is
/// launched; progress arrives via `engine-info` / `engine-done` events.
#[tauri::command]
async fn analyze_position(
    app: tauri::AppHandle,
    state: State<'_, EngineState>,
    fen: String,
    nodes: Option<u64>,
    user_path: Option<String>,
) -> Result<(), String> {
    let path = uci::resolve_engine_path(user_path.as_deref())?;
    let nodes = nodes.unwrap_or(uci::DEFAULT_NODES).max(1);
    let engine_slot = Arc::clone(&state.engine);

    tauri::async_runtime::spawn(async move {
        let done = run_search(&app, &engine_slot, path, fen, nodes).await;
        let payload = match done {
            Ok(best) => DonePayload {
                bestmove: Some(best.bestmove),
                ponder: best.ponder,
                error: None,
            },
            Err(e) => DonePayload {
                bestmove: None,
                ponder: None,
                error: Some(e),
            },
        };
        let _ = app.emit("engine-done", payload);
    });
    Ok(())
}

/// Interrupt the current search (engine replies `bestmove` promptly).
/// No-op if nothing is running.
#[tauri::command]
fn stop_analysis(state: State<'_, EngineState>) -> Result<(), String> {
    let handle = state.stop.lock().expect("stop mutex poisoned").clone();
    if let Some(handle) = handle {
        tauri::async_runtime::spawn(async move {
            let _ = handle.stop().await;
        });
    }
    Ok(())
}

/// Ensure an engine at `path` is running (respawning if the path changed or
/// the previous process died), then run one bounded search, streaming infos.
async fn run_search(
    app: &tauri::AppHandle,
    engine_slot: &Arc<Mutex<Option<Engine>>>,
    path: std::path::PathBuf,
    fen: String,
    nodes: u64,
) -> Result<uci::BestMove, String> {
    let mut slot = engine_slot.lock().await;
    let needs_spawn = match slot.as_ref() {
        Some(engine) => engine.path() != path,
        None => true,
    };
    if needs_spawn {
        if let Some(old) = slot.take() {
            old.quit().await;
        }
        let engine = Engine::spawn(&path).await?;
        let state: State<'_, EngineState> = app.state();
        *state.stop.lock().expect("stop mutex poisoned") = Some(engine.stop_handle());
        *slot = Some(engine);
    }
    let engine = slot.as_mut().expect("engine just ensured");
    let result = engine
        .analyze(&UciPosition::Fen(fen), nodes, |info| {
            // Skip currmove/progress lines that carry no evaluation.
            if info.has_score() {
                let _ = app.emit("engine-info", info);
            }
        })
        .await;
    if result.is_err() {
        // Engine likely died; drop it so the next analyze respawns.
        *slot = None;
        let state: State<'_, EngineState> = app.state();
        *state.stop.lock().expect("stop mutex poisoned") = None;
    }
    result
}

/// Build and run the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .manage(EngineState::default())
        .manage(browse::DbState::default())
        .manage(dbops::JobsWorker::default())
        .manage(endgame::EndgameState::default())
        .invoke_handler(tauri::generate_handler![
            resolve_engine_path,
            analyze_position,
            stop_analysis,
            browse::open_database,
            browse::list_games,
            browse::get_game,
            browse::opening_tree,
            browse::find_games_at,
            browse::eco_names,
            prep::matching_players,
            prep::prep_view,
            prep::prep_fingerprint,
            tokens::get_game_tokens,
            tokens::update_game_tokens,
            explain::explain_position,
            tactics::tactics_state,
            tactics::tactics_import_puzzles,
            tactics::tactics_next_puzzle,
            tactics::tactics_verify_move,
            tactics::tactics_record_attempt,
            tactics::tactics_woodpecker_sets,
            tactics::tactics_create_woodpecker_set,
            tactics::tactics_woodpecker_puzzles,
            tactics::tactics_start_cycle,
            tactics::tactics_finish_cycle,
            tactics::tactics_cycle_stats,
            endgame::endgame_overview,
            endgame::endgame_start,
            endgame::endgame_move,
            endgame::endgame_give_up,
            train::train_summary,
            train::train_queue,
            train::train_grade,
            train::train_add_line,
            dbops::game_analyses,
            dbops::annotate_game,
            dbops::reanalyze_game,
            dbops::run_jobs,
            dbops::jobs_status,
            dbops::batch_estimate,
            dbops::batch_start,
            dbops::batch_pause,
            dbops::export_game_pgn,
            dbops::build_profile,
            dbops::get_narration_voice,
            dbops::set_narration_voice,
            dbops::set_window_title,
            home::home_summary,
            home::touch_last_game,
            home::cache_profile,
            home::commitment_get,
            home::commitment_set,
            home::prep_state_get,
            home::prep_state_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
