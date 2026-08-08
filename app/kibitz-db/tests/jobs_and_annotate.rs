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

/// THE engine-off assertion: annotating a featureless game (no screens,
/// no plans, so nothing closing-eligible) must spawn ZERO engines and
/// enqueue ZERO jobs — and running the (empty) queue must still spawn
/// nothing, even with a bogus engine path (lazy spawn).
#[test]
fn engine_stays_off_for_a_featureless_game() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    const FEATURELESS: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 *\n";
    let (_dir, conn) = setup(FEATURELESS);
    let spawns_before = spawn_count();
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 200_000, 12).unwrap();
    assert_eq!(report.jobs_enqueued, 0, "featureless: no confirm jobs");
    assert_eq!(
        report.suggest_jobs_enqueued, 0,
        "no plans, nothing to verify"
    );
    assert_eq!(report.screens_fired, 0);
    let run = jobs::run_pending(&conn, std::path::Path::new("/nonexistent-engine"), 100).unwrap();
    assert_eq!((run.done, run.failed), (0, 0));
    assert_eq!(
        spawn_count(),
        spawns_before,
        "no engine process may be spawned for a featureless game"
    );
}

/// A quiet game WITH plan plies (2026-07-29 field report): annotate
/// enqueues suggest-verify jobs for the closing-eligible plies — but
/// still fires no screens, enqueues no confirms, and spawns NO engine
/// (enqueue-only; the worker the user starts does the searching).
#[test]
fn quiet_plan_game_enqueues_suggest_verify_without_spawning() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    const QUIET: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 Nf6 5. Nc3 d6 *\n";
    let (_dir, conn) = setup(QUIET);
    let spawns_before = spawn_count();
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 200_000, 12).unwrap();
    assert_eq!(report.screens_fired, 0);
    assert_eq!(report.jobs_enqueued, 0, "no screens, no confirms");
    assert!(
        report.suggest_jobs_enqueued > 0,
        "the plan plies are closing-eligible: {report:?}"
    );
    let (pending, running, done, failed) = jobs::counts(&conn).unwrap();
    assert_eq!(pending as u32, report.suggest_jobs_enqueued);
    assert_eq!((running, done, failed), (0, 0, 0));
    assert_eq!(spawn_count(), spawns_before, "enqueue must not spawn");
    // A quiet game legitimately earns modest commentary; if any was
    // added it must have persisted.
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
    assert_eq!(
        pending as u32,
        report.jobs_enqueued + report.suggest_jobs_enqueued
    );
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
    // Two distinct featureless games (different movetext, so no dup
    // collapse): no screens AND no plan plies = no engine jobs of any
    // kind = engine never needed.
    const TWO_QUIET: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
        1. e4 e5 2. Nf3 Nc6 *\n\n\
        [White \"C\"]\n[Black \"D\"]\n[Result \"*\"]\n\n\
        1. d4 d5 2. Nf3 Nf6 *\n";
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
    assert_eq!(
        v["suggest_jobs_enqueued"].as_u64(),
        Some(0),
        "featureless games have no closing-eligible plies: {v}"
    );
}

/// QGD Exchange, minority-attack shape: a strategic middlegame with many
/// quiet plan plies (the 2026-07-29 field-report shape — plans narrate,
/// but no screen fires, so no wsui-confirm job ever reviewed the
/// suggestions and closings never rendered). Probed ply map: 1-6 no
/// plans; 7, 8, 26, 29 capture plies; 24, 25, 28 fired screens; the rest
/// quiet plan plies.
const QGD: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
    1. d4 d5 2. c4 e6 3. Nc3 Nf6 4. cxd5 exd5 5. Bg5 Be7 6. e3 c6 \
    7. Bd3 Nbd7 8. Qc2 O-O 9. Nf3 Re8 10. O-O Nf8 11. Rab1 a5 \
    12. a3 Ne4 13. Bxe7 Qxe7 14. b4 axb4 15. axb4 *\n";

fn plies_of(conn: &rusqlite::Connection, purpose: &str) -> Vec<u32> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT json_extract(payload, '$.ply') FROM jobs
             WHERE purpose = ?1 ORDER BY 1",
        )
        .unwrap();
    let rows = stmt.query_map([purpose], |r| r.get::<_, u32>(0)).unwrap();
    rows.collect::<Result<_, _>>().unwrap()
}

/// Eligibility (2026-07-29): a quiet plan ply gets a suggest-verify job;
/// fired plies get their review via wsui-confirm instead (no duplicate);
/// capture plies and plan-less plies are excluded; re-annotating is
/// idempotent (json_extract dedup on game AND ply); nothing spawns.
#[test]
fn annotate_enqueues_suggest_verify_only_at_quiet_plan_plies() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (_dir, conn) = setup(QGD);
    let spawns_before = spawn_count();
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    assert!(report.suggest_jobs_enqueued > 0, "{report:?}");

    let sv = plies_of(&conn, "suggest-verify");
    let confirms = plies_of(&conn, "wsui-confirm");
    assert!(sv.contains(&9), "quiet plan ply 9 is eligible: {sv:?}");
    assert!(sv.contains(&27), "quiet plan ply 27 is eligible: {sv:?}");
    for fired in [24, 25, 28] {
        assert!(
            !sv.contains(&fired),
            "fired ply {fired} gets its review via wsui-confirm, not a duplicate: {sv:?}"
        );
        assert!(confirms.contains(&fired), "{confirms:?}");
    }
    for capture in [7, 8, 26, 29] {
        assert!(!sv.contains(&capture), "capture ply {capture}: {sv:?}");
    }
    // The PROPERTY, not a list of ply indices. `for planless in 1..=6`
    // was a true statement about a pipeline that filtered plans away, and
    // editing the list to match new output would be a test rewritten to
    // fit the data — it would then pass forever regardless of what
    // happened to plan generation.
    //
    // What must hold is the reason: a ply is skipped because the position
    // offers the side to move no plan, and any ply that DOES get a job
    // has one. If a ply stops being plan-less next month for a good
    // reason this notices and says which; if it stops because plan
    // generation broke, the same assertion fails.
    let start = cozy_chess::Board::default();
    let movetext: Vec<u8> = conn
        .query_row("SELECT movetext FROM games WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    let moves = kibitz_db::movebin::decode_game(&start, &movetext).unwrap();
    for ply_idx in 1..=moves.len() {
        let ply = ply_idx as u32;
        let mut b = start.clone();
        for &mv in &moves[..ply_idx] {
            b.play(mv);
        }
        let record = kibitz_core::analyze(&b);
        let stm = match b.side_to_move() {
            cozy_chess::Color::White => kibitz_core::record::Favors::White,
            cozy_chess::Color::Black => kibitz_core::record::Favors::Black,
        };
        let has_plan_for_mover = record
            .composite_plans
            .iter()
            .any(|c| c.favors == stm || c.favors == kibitz_core::record::Favors::Balanced);
        if sv.contains(&ply) {
            assert!(
                has_plan_for_mover,
                "ply {ply} got a suggest-verify job with no composite plan for the \
                 side to move — the gate is enqueueing engine work with nothing to \
                 spend it on: {sv:?}"
            );
        } else if !confirms.contains(&ply) {
            // Not fired, not enqueued: either no plan for the mover, a
            // capture, or suggest produced nothing. Only the first is
            // asserted here; the others have their own checks above.
            let capture = [7u32, 8, 26, 29].contains(&ply);
            assert!(
                !has_plan_for_mover || capture,
                "ply {ply} has a plan for the side to move and was skipped anyway: {sv:?}"
            );
        }
    }
    assert_eq!(sv.len() as u32, report.suggest_jobs_enqueued);

    // Idempotent: a re-annotate reuses every existing suggest-verify job.
    let again = kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    assert_eq!(again.suggest_jobs_enqueued, 0, "dedup by (game_id, ply)");
    assert_eq!(plies_of(&conn, "suggest-verify"), sv);
    assert_eq!(spawn_count(), spawns_before, "enqueue must not spawn");
}

/// The worker's suggest-verify branch with nothing to verify: a position
/// whose static suggester proposes no moves completes the job with an
/// honest empty cleared list and NEVER spawns the engine — and the
/// verdict loader is tolerant of the status-less row.
#[test]
fn suggest_verify_with_nothing_to_verify_completes_without_engine() {
    let _g = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    // The symmetrical open-game start: no imbalance, no plans, so the
    // static suggester proposes nothing.
    let featureless = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
    let (_id, created) = jobs::enqueue_suggest_verify(&conn, 42, 7, featureless).unwrap();
    assert!(created);
    let spawns_before = spawn_count();
    let run = jobs::run_pending(&conn, std::path::Path::new("/nonexistent-engine"), 10).unwrap();
    assert_eq!((run.done, run.failed), (1, 0));
    assert_eq!(spawn_count(), spawns_before, "no suggestions, no engine");
    let verdicts = kibitz_db::narrate::load_verdicts(&conn, 42).unwrap();
    let v = verdicts.get(&7).expect("the completed review loads");
    assert!(
        v.status.is_none(),
        "a suggestion review has no confirm status"
    );
    assert_eq!(v.cleared_suggestions, Some(vec![]));
}

/// Complete every pending suggest-verify job WITHOUT an engine: recompute
/// the payload position's static suggestions and store them all cleared
/// (`clear_all`) or all refuted (empty cleared list) in the same JSON
/// shape the worker writes.
fn plant_suggest_results(conn: &rusqlite::Connection, clear_all: bool) {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, payload FROM jobs WHERE purpose='suggest-verify' AND status='pending'",
            )
            .unwrap();
        let r = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        r.collect::<Result<_, _>>().unwrap()
    };
    assert!(!rows.is_empty(), "expected pending suggest-verify jobs");
    for (id, payload) in rows {
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let cleared: Vec<String> = if clear_all {
            let board: cozy_chess::Board = p["fen"].as_str().unwrap().parse().unwrap();
            let record = kibitz_core::analyze(&board);
            kibitz_core::suggest::suggest(&record, &board)
                .iter()
                .map(|s| s.mv.clone())
                .collect()
        } else {
            Vec::new()
        };
        let result = serde_json::json!({
            "game_id": p["game_id"],
            "ply": p["ply"],
            "cleared_suggestions": cleared,
            "verify_nodes": 150000,
            "engine": "planted",
        });
        conn.execute(
            "UPDATE jobs SET status='done', result=?1 WHERE id=?2",
            rusqlite::params![result.to_string(), id],
        )
        .unwrap();
    }
}

fn is_closing(text: &str) -> bool {
    // The coach-voice closing templates (templates/coach.tmpl): the
    // constructive "If I had the move: ..." and the prophylactic
    // "First, deny the opponent: ...".
    text.contains("If I had the move") || text.contains("First, deny the opponent")
}

/// End to end (2026-07-29 field report): once the suggestion reviews
/// complete and fold back, the narration at a quiet plan ply carries a
/// closing built from the engine-cleared candidates — the annotated game
/// actually recommends a move.
#[test]
fn cleared_suggest_verdicts_fold_back_into_a_closing() {
    let (_dir, conn) = setup(QGD);
    let report = kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    assert!(report.suggest_jobs_enqueued > 0);
    plant_suggest_results(&conn, true);
    let f = kibitz_db::annotate::fold_back(&conn).unwrap();
    assert!(f.folded > 0);
    // Suggest-verify rows are NOT confirm verdicts: the alert grading
    // counters stay untouched (the confirm jobs are still pending here).
    assert_eq!((f.confirmed, f.refuted, f.unclear), (0, 0, 0));

    let sv = plies_of(&conn, "suggest-verify");
    let narrations = kibitz_db::narrate::narrations(&conn, 1).unwrap();
    assert!(
        sv.iter()
            .any(|ply| narrations.get(ply).is_some_and(|t| is_closing(t))),
        "a cleared quiet plan ply must recommend a move.\nplies: {sv:?}\nnarrations: {narrations:#?}"
    );

    // Idempotent: a second fold has nothing left to do.
    let again = kibitz_db::annotate::fold_back(&conn).unwrap();
    assert_eq!(again.folded, 0);
}

/// The safety property survives: when the engine refutes every candidate
/// at the reviewed plies, no closing renders there — refuted moves never
/// appear as recommendations.
#[test]
fn refuted_only_suggest_verdicts_yield_no_closing() {
    let (_dir, conn) = setup(QGD);
    kibitz_db::annotate::annotate_game(&conn, 1, 150_000, 12).unwrap();
    plant_suggest_results(&conn, false);
    kibitz_db::annotate::fold_back(&conn).unwrap();
    let sv = plies_of(&conn, "suggest-verify");
    let narrations = kibitz_db::narrate::narrations(&conn, 1).unwrap();
    for ply in &sv {
        if let Some(text) = narrations.get(ply) {
            assert!(
                !is_closing(text),
                "refuted candidates must not be recommended at ply {ply}: {text}"
            );
        }
    }
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
            "SELECT result FROM jobs
             WHERE purpose='wsui-confirm' AND status='done' ORDER BY id LIMIT 1",
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
    // The quiet plan plies' suggest-verify jobs drained in the same run,
    // each storing a cleared list in the wsui-confirm shape.
    let sv: String = conn
        .query_row(
            "SELECT result FROM jobs
             WHERE purpose='suggest-verify' AND status='done' ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("the trap's quiet plan plies enqueue suggest-verify jobs");
    let v: serde_json::Value = serde_json::from_str(&sv).unwrap();
    assert!(v["cleared_suggestions"].is_array(), "{v}");
}
