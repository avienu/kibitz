//! Verification-bar tests: the engine-off product principle (CLAUDE.md
//! #6), queue resumability, and gated live confirmation.

use std::io::Cursor;
use std::sync::Mutex;

use kibitz_db::engine::{resolve_engine_path, spawn_count};

/// spawn_count() is process-global; tests that assert on it must not
/// overlap with the live test that really spawns an engine.
static SPAWN_LOCK: Mutex<()> = Mutex::new(());
use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
use kibitz_db::jobs;

fn setup(pgn: &str) -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let src = SourceInfo {
        name: "t".into(),
        origin: "test".into(),
        license: "test".into(),
        kind: SourceKind::Personal,
    };
    let st = import_pgn(&conn, &src, Cursor::new(pgn)).unwrap();
    assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);
    (dir, conn)
}

/// THE engine-off assertion: annotating a quiet game must spawn ZERO
/// engines and enqueue ZERO jobs — and running the (empty) queue must
/// still spawn nothing, even with a bogus engine path (lazy spawn).
#[test]
fn engine_stays_off_for_a_quiet_game() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    const QUIET: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 Nf6 5. Nc3 d6 *\n";
    let (_dir, conn) = setup(QUIET);
    let spawns_before = spawn_count();
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 200_000, 12).unwrap();
    assert_eq!(report.jobs_enqueued, 0, "quiet game must enqueue nothing");
    assert_eq!(report.screens_fired, 0);
    let run = jobs::run_pending(&conn, std::path::Path::new("/nonexistent-engine"), 100).unwrap();
    assert_eq!((run.done, run.failed), (0, 0));
    assert_eq!(
        spawn_count(),
        spawns_before,
        "no engine process may be spawned for a quiet game"
    );
    // A featureless five-mover legitimately earns no commentary; if any
    // was added it must have persisted.
    let pgn = kibitz_db::export::export_pgn(&conn, 1).unwrap();
    if report.comments_added > 0 {
        assert!(pgn.contains('{'), "comments persisted: {pgn}");
    }
}

/// A fired screen enqueues exactly one bounded job for that position;
/// nothing runs until a worker is invoked.
#[test]
fn fired_screen_enqueues_but_does_not_run() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Blackburne Shilling Gambit: after 4.Nxe5?? Qg5 the e5 knight is
    // loose and attacked (source: standard trap theory).
    const TRAP: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 4. Nxe5 Qg5 *\n";
    let (_dir, conn) = setup(TRAP);
    let spawns_before = spawn_count();
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    assert!(report.screens_fired > 0, "{report:?}");
    assert!(report.jobs_enqueued > 0);
    let (pending, running, done, failed) = jobs::counts(&conn).unwrap();
    assert_eq!(pending as u32, report.jobs_enqueued);
    assert_eq!((running, done, failed), (0, 0, 0));
    assert_eq!(spawn_count(), spawns_before, "enqueue must not spawn");
}

/// Resumability: 'running' rows revert to 'pending' on worker startup.
#[test]
fn queue_resets_running_jobs_on_startup() {
    const TRAP: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 4. Nxe5 Qg5 *\n";
    let (_dir, conn) = setup(TRAP);
    kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    conn.execute("UPDATE jobs SET status='running'", [])
        .unwrap();
    let reset = jobs::reset_running(&conn).unwrap();
    assert!(reset > 0);
    let (pending, running, _, _) = jobs::counts(&conn).unwrap();
    assert!(pending > 0);
    assert_eq!(running, 0);
}

/// Round-2 item 6: batch-annotate jobs are enqueued idempotently, execute
/// STATICALLY inside the worker (zero engine spawns for quiet games), and
/// the stop flag pauses between jobs leaving the rest pending — pause =
/// stop the worker, the queue is the resumable state.
#[test]
fn batch_annotate_runs_statically_and_pauses_between_jobs() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Two distinct quiet games (different movetext, so no dup collapse):
    // quiet = no WSUI screen fires = no confirm jobs = engine never needed.
    const TWO_QUIET: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 Nf6 5. Nc3 d6 *\n\n\
        [White \"C\"]\n[Black \"D\"]\n[Result \"*\"]\n\n\
        1. d4 d5 2. Nf3 Nf6 3. e3 e6 4. Bd3 Bd6 5. O-O O-O *\n";
    let dir = tempfile::tempdir().unwrap();
    let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let src = SourceInfo {
        name: "t".into(),
        origin: "test".into(),
        license: "test".into(),
        kind: SourceKind::Personal,
    };
    let st = import_pgn(&conn, &src, Cursor::new(TWO_QUIET)).unwrap();
    assert_eq!(st.games_imported, 2, "failures: {:?}", st.failures);
    let spawns_before = spawn_count();

    // Enqueue one job per game; re-starting skips covered games.
    assert_eq!(jobs::enqueue_batch_annotate(&conn, 200_000, 12).unwrap(), 2);
    assert_eq!(jobs::enqueue_batch_annotate(&conn, 200_000, 12).unwrap(), 0);

    // A raised stop flag pauses before anything starts.
    let stop = AtomicBool::new(true);
    let path = std::path::Path::new("/nonexistent-engine");
    let r = jobs::run_pending_until(&conn, path, 100, Some(&stop)).unwrap();
    assert_eq!((r.done, r.failed), (0, 0));
    assert_eq!(jobs::counts(&conn).unwrap().0, 2, "both still pending");

    // Run one job, then stop again: the second job stays pending.
    stop.store(false, Ordering::SeqCst);
    let r = jobs::run_pending_until(&conn, path, 1, Some(&stop)).unwrap();
    assert_eq!((r.done, r.failed), (1, 0));
    let (pending, running, done, failed) = jobs::counts(&conn).unwrap();
    assert_eq!((pending, running, done, failed), (1, 0, 1, 0));

    // Resume drains the rest. Quiet games fire no screens, so the whole
    // batch completes with the bogus engine path untouched.
    let r = jobs::run_pending(&conn, path, 100).unwrap();
    assert_eq!((r.done, r.failed), (1, 0));
    assert_eq!(jobs::counts(&conn).unwrap().0, 0);
    assert_eq!(
        spawn_count(),
        spawns_before,
        "static batch annotate must never spawn an engine"
    );

    // Each job stored an honest static result.
    let result: String = conn
        .query_row(
            "SELECT result FROM jobs WHERE purpose='batch-annotate' AND status='done'
             ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(v["positions_analyzed"].as_u64().unwrap() > 0, "{v}");
    assert_eq!(v["jobs_enqueued"].as_u64(), Some(0), "quiet games: {v}");
}

/// Live (skipped when no engine binary is available): the Winawer field
/// report end to end. After 5.a3 the WSUI screen fires (the b4-bishop
/// hangs), every static suggestion is marked, and the confirm job's
/// cursory suggestion review must refute the piece-droppers f5/f6 while
/// clearing the theory move cxd4 (axb4 is met by dxc3).
#[test]
fn live_verification_clears_theory_and_refutes_droppers() {
    let Some(engine) = resolve_engine_path() else {
        eprintln!("skipping live_verification_clears_theory_and_refutes_droppers: no engine");
        return;
    };
    const WINAWER: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e6 2. d4 d5 3. Nc3 Bb4 4. e5 c5 5. a3 *\n";
    let (_dir, conn) = setup(WINAWER);
    kibitz_db::annotate::annotate_game(&conn, 1, 100_000, 12).unwrap();
    {
        let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let run = jobs::run_pending(&conn, &engine, 100).unwrap();
        assert!(run.done > 0, "{run:?}");
        assert_eq!(run.failed, 0);
    }
    let verdicts = kibitz_db::narrate::load_verdicts(&conn, 1).unwrap();
    let v = verdicts.get(&9).expect("5.a3 is mainline ply 9");
    let cleared = v
        .cleared_suggestions
        .as_ref()
        .expect("the fired screen carries a suggestion review");
    assert!(
        !cleared.iter().any(|u| u == "f7f5" || u == "f7f6"),
        "the piece-droppers must be refuted: {cleared:?}"
    );
    assert!(
        cleared.iter().any(|u| u == "c5d4"),
        "the theory move cxd4 must be engine-cleared: {cleared:?}"
    );
}

/// Live (skipped when no engine binary is available, e.g. Linux CI):
/// running the queue spawns exactly ONE engine for N jobs and grades the
/// fired alert.
#[test]
fn live_confirmation_grades_the_alert() {
    let Some(engine) = resolve_engine_path() else {
        eprintln!("skipping live_confirmation_grades_the_alert: no engine binary");
        return;
    };
    const TRAP: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 4. Nxe5 Qg5 *\n";
    let (_dir, conn) = setup(TRAP);
    kibitz_db::annotate::annotate_game(&conn, 1, 100_000, 12).unwrap();
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let spawns_before = spawn_count();
    let run = jobs::run_pending(&conn, &engine, 100).unwrap();
    assert!(run.done > 0, "{run:?}");
    assert_eq!(run.failed, 0);
    assert_eq!(
        spawn_count() - spawns_before,
        1,
        "one engine process serves the whole batch"
    );
    let result: String = conn
        .query_row(
            "SELECT result FROM jobs WHERE status='done' ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        ["confirmed", "refuted", "unclear-at-budget"].contains(&v["status"].as_str().unwrap_or("")),
        "graded: {v}"
    );
    assert!(v["pv"].as_array().is_some_and(|a| !a.is_empty()));
}
