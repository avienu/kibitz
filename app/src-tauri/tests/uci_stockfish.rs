//! Integration test against a real Stockfish binary.
//!
//! Uses the repo-local engine at tools/stockfish/ (see CLAUDE.md / ROADMAP
//! Phase 0). Skips gracefully when the binary is absent (e.g. Linux CI)
//! so `cargo test` stays green on machines without the macOS binary.

use std::path::PathBuf;

use kibitz_app_lib::uci::{Engine, UciPosition};

fn repo_engine_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/stockfish/stockfish-macos-m1-apple-silicon")
}

#[tokio::test]
async fn stockfish_startpos_go_nodes_reports_info_and_bestmove() {
    let path = repo_engine_path();
    if !path.is_file() {
        eprintln!(
            "SKIP: Stockfish binary not found at {} — integration test not run.",
            path.display()
        );
        return;
    }

    // Engine::spawn performs the `uci` -> `uciok` and `isready` -> `readyok`
    // handshake; a failure there fails the test.
    let mut engine = Engine::spawn(&path).await.expect("spawn + uci handshake");

    let mut infos = Vec::new();
    let best = engine
        .analyze(&UciPosition::Startpos, Some(100_000), |info| {
            infos.push(info)
        })
        .await
        .expect("search should complete with a bestmove");

    // bestmove must look like a UCI move (e2e4, g1f3, possibly a promotion).
    assert!(
        best.bestmove.len() == 4 || best.bestmove.len() == 5,
        "unexpected bestmove: {:?}",
        best.bestmove
    );
    assert!(
        best.bestmove.as_bytes()[0].is_ascii_lowercase(),
        "unexpected bestmove: {:?}",
        best.bestmove
    );

    // At least one parsed info line must carry a cp score, a depth and a pv.
    assert!(
        infos
            .iter()
            .any(|i| i.score_cp.is_some() && i.depth.is_some() && i.pv.is_some()),
        "no info line with cp score + depth + pv; got {} info lines",
        infos.len()
    );

    engine.quit().await;
}

#[tokio::test]
async fn stop_handle_interrupts_a_long_search() {
    let path = repo_engine_path();
    if !path.is_file() {
        eprintln!("SKIP: Stockfish binary not found; integration test not run.");
        return;
    }

    let mut engine = Engine::spawn(&path).await.expect("spawn + uci handshake");
    let stop = engine.stop_handle();

    // Huge node budget; without `stop` this would run for a very long time.
    let position =
        UciPosition::Fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into());
    // Interruptibility now exercises the live-analysis path: go infinite.
    let search = engine.analyze(&position, None, |_| {});
    let stopper = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        stop.stop().await.expect("stop write");
    };
    let (result, ()) = tokio::join!(search, stopper);
    let best = result.expect("interrupted search still yields bestmove");
    assert!(!best.bestmove.is_empty());

    engine.quit().await;
}
