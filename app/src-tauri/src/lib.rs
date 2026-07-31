//! Kibitz Tauri shell.
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
pub mod explorer;
pub mod home;
mod identity;
pub mod lab;
pub mod lichess_play;
pub mod netops;
pub mod prep;
pub mod session;
pub mod tactics;
pub mod tokens;
pub mod train;
pub mod triage;
pub mod uci;
pub mod updates;
pub mod verify;

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
    pub(crate) engine: Arc<Mutex<Option<Engine>>>,
    pub(crate) stop: std::sync::Mutex<Option<StopHandle>>,
}

/// Ensure `slot` holds a live engine spawned from `path` (respawning if
/// the path changed), registering its stop handle in `stop_slot`. Shared
/// by `analyze_position` and `verify_suggestions`.
pub(crate) async fn ensure_engine(
    slot: &mut Option<Engine>,
    stop_slot: &std::sync::Mutex<Option<StopHandle>>,
    path: &std::path::Path,
) -> Result<(), String> {
    let needs_spawn = match slot.as_ref() {
        Some(engine) => engine.path() != path,
        None => true,
    };
    if needs_spawn {
        if let Some(old) = slot.take() {
            old.quit().await;
        }
        let engine = Engine::spawn(path).await?;
        *stop_slot.lock().expect("stop mutex poisoned") = Some(engine.stop_handle());
        *slot = Some(engine);
    }
    Ok(())
}

/// Streamed `engine-info` payload: the parsed UCI info line PLUS the FEN
/// of the position the search was started on. The frontend must attribute
/// every eval/PV to this fen — never to "whatever position is currently
/// shown" — otherwise infos still streaming from a just-stopped search get
/// stamped with the new position and the score renders with a flipped
/// sign (audit 2026-07 #5).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoPayload {
    fen: String,
    #[serde(flatten)]
    info: uci::UciInfo,
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
/// Resolution order: user path > KIBITZ_STOCKFISH > repo binary > PATH.
#[tauri::command]
fn resolve_engine_path(user_path: Option<String>) -> Result<String, String> {
    uci::resolve_engine_path(user_path.as_deref()).map(|p| p.display().to_string())
}

/// Validate an engine binary by running the `uci` handshake and return
/// its `id name` (Settings' engine manager). No search is started — this
/// is an explicit user action, so the engine-off default is untouched.
#[tauri::command]
async fn engine_identify(user_path: Option<String>) -> Result<uci::EngineIdentity, String> {
    let path = uci::resolve_engine_path(user_path.as_deref())?;
    uci::identify(&path).await
}

/// Start `go nodes <nodes>` on `fen`. Returns as soon as the search task is
/// launched; progress arrives via `engine-info` / `engine-done` events.
#[tauri::command]
async fn analyze_position(
    app: tauri::AppHandle,
    state: State<'_, EngineState>,
    fen: String,
    nodes: Option<u64>,
    infinite: Option<bool>,
    user_path: Option<String>,
) -> Result<(), String> {
    let path = uci::resolve_engine_path(user_path.as_deref())?;
    // Live analysis (run-8 ruling): `infinite` runs `go infinite`, ended
    // only by stop_analysis — an explicit user action either way, so the
    // engine-off principle (which governs defaults) is untouched.
    let nodes = if infinite.unwrap_or(false) {
        None
    } else {
        Some(nodes.unwrap_or(uci::DEFAULT_NODES).max(1))
    };
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
    nodes: Option<u64>,
) -> Result<uci::BestMove, String> {
    let mut slot = engine_slot.lock().await;
    {
        let state: State<'_, EngineState> = app.state();
        ensure_engine(&mut slot, &state.stop, &path).await?;
    }
    let engine = slot.as_mut().expect("engine just ensured");
    let searched_fen = fen.clone();
    let result = engine
        .analyze(&UciPosition::Fen(fen), nodes, |info| {
            // Skip currmove/progress lines that carry no evaluation.
            if info.has_score() {
                let _ = app.emit(
                    "engine-info",
                    InfoPayload {
                        fen: searched_fen.clone(),
                        info,
                    },
                );
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(EngineState::default())
        .manage(browse::DbState::default())
        .manage(dbops::JobsWorker::default())
        .manage(endgame::EndgameState::default())
        .manage(netops::NetWorker::default())
        .manage(lichess_play::PlayState::default())
        .invoke_handler(tauri::generate_handler![
            resolve_engine_path,
            engine_identify,
            analyze_position,
            stop_analysis,
            browse::open_database,
            browse::create_database,
            browse::db_summary,
            session::last_database,
            session::migrate_database_to_app_storage,
            session::ui_session_get,
            session::ui_session_set,
            home::last_game_get,
            home::self_player_get,
            home::self_player_set,
            browse::list_games,
            browse::get_game,
            browse::opening_tree,
            browse::find_games_at,
            browse::eco_names,
            browse::crosstable_games,
            explorer::explorer_fetch,
            prep::matching_players,
            prep::prep_view,
            prep::prep_fingerprint,
            tokens::get_game_tokens,
            tokens::update_game_tokens,
            explain::explain_position,
            verify::verify_suggestions,
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
            endgame::tablebase_status,
            endgame::set_tablebase_dir,
            endgame::endgame_start,
            endgame::endgame_move,
            endgame::endgame_give_up,
            train::train_summary,
            train::train_queue,
            train::train_grade,
            train::train_add_line,
            triage::triage_report,
            triage::triage_extend,
            triage::triage_extension_status,
            lab::lab_cohorts,
            lab::lab_report,
            lab::lab_line_fit,
            lab::lab_reanalyze_estimate,
            lab::lab_reanalyze_start,
            dbops::game_analyses,
            dbops::annotate_game,
            dbops::reanalyze_game,
            dbops::run_jobs,
            dbops::jobs_status,
            dbops::batch_estimate,
            dbops::batch_start,
            dbops::batch_pause,
            dbops::repertoire_marks,
            dbops::export_game_pgn,
            dbops::build_profile,
            dbops::get_narration_voice,
            dbops::set_narration_voice,
            dbops::set_window_title,
            home::home_summary,
            home::touch_last_game,
            identity::identity_group,
            identity::alias_declare,
            identity::alias_remove,
            home::cache_profile,
            home::commitment_get,
            home::commitment_set,
            home::prep_state_get,
            home::prep_state_set,
            updates::update_check,
            netops::twic_catalog,
            netops::twic_refresh_catalog,
            netops::twic_download,
            netops::twic_set_auto_sync,
            netops::twic_ack_notice,
            netops::twic_auto_sync_check,
            netops::sync_accounts,
            netops::sync_set_username,
            netops::sync_run,
            netops::net_progress,
            netops::net_cancel,
            netops::rail_net_badges,
            lichess_play::lichess_token_set,
            lichess_play::lichess_token_clear,
            lichess_play::lichess_token_status,
            lichess_play::lichess_play_start,
            lichess_play::lichess_play_join,
            lichess_play::lichess_play_move,
            lichess_play::lichess_play_resign,
            lichess_play::lichess_play_abort,
            lichess_play::lichess_play_draw,
            lichess_play::lichess_play_seek,
            lichess_play::lichess_seek_cancel,
            lichess_play::lichess_now_playing
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audit 2026-07 #5: every streamed info carries the searched FEN so
    /// the frontend can attribute the (side-to-move POV) score to the
    /// right position; the score itself passes through unflipped.
    #[test]
    fn info_payload_carries_searched_fen_with_flattened_info() {
        let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let payload = InfoPayload {
            fen: fen.to_string(),
            info: uci::UciInfo {
                depth: Some(20),
                score_cp: Some(-258),
                ..Default::default()
            },
        };
        let v = serde_json::to_value(&payload).expect("serializes");
        assert_eq!(v["fen"], fen);
        // Flattened camelCase fields — the TS EngineInfo contract.
        assert_eq!(v["depth"], 20);
        assert_eq!(v["scoreCp"], -258);
        assert!(v.get("scoreMate").is_none());
    }
}
