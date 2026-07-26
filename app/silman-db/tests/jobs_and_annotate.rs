//! Verification-bar tests: the engine-off product principle (CLAUDE.md
//! #6), queue resumability, and gated live confirmation.

use std::io::Cursor;

use silman_db::engine::{resolve_engine_path, spawn_count};
use silman_db::import::{import_pgn, SourceInfo, SourceKind};
use silman_db::jobs;

fn setup(pgn: &str) -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
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
    const QUIET: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 Nf6 5. Nc3 d6 *\n";
    let (_dir, conn) = setup(QUIET);
    let spawns_before = spawn_count();
    let report = silman_db::annotate::annotate_game(&conn, 1, 200_000, 12).unwrap();
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
    let pgn = silman_db::export::export_pgn(&conn, 1).unwrap();
    if report.comments_added > 0 {
        assert!(pgn.contains('{'), "comments persisted: {pgn}");
    }
}

/// A fired screen enqueues exactly one bounded job for that position;
/// nothing runs until a worker is invoked.
#[test]
fn fired_screen_enqueues_but_does_not_run() {
    // Blackburne Shilling Gambit: after 4.Nxe5?? Qg5 the e5 knight is
    // loose and attacked (source: standard trap theory).
    const TRAP: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 4. Nxe5 Qg5 *\n";
    let (_dir, conn) = setup(TRAP);
    let spawns_before = spawn_count();
    let report = silman_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
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
    silman_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    conn.execute("UPDATE jobs SET status='running'", [])
        .unwrap();
    let reset = jobs::reset_running(&conn).unwrap();
    assert!(reset > 0);
    let (pending, running, _, _) = jobs::counts(&conn).unwrap();
    assert!(pending > 0);
    assert_eq!(running, 0);
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
    silman_db::annotate::annotate_game(&conn, 1, 100_000, 12).unwrap();
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
