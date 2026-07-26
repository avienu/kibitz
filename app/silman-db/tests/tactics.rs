//! Tactics trainer integration tests over the committed CC0 fixture
//! `testdata/fixtures/puzzles_sample.csv` (500 rows of the Lichess puzzle
//! dump). Ground-truth numbers cited in asserts were computed directly
//! from the CSV with awk: 500 rows, 290 with Popularity >= 90, ratings
//! 424..=2785, 63 puzzles rated 1400..=1600, 103 puzzles carrying an
//! Undefended-mapped theme (hangingPiece/fork/discoveredAttack/skewer).

use std::fs::File;
use std::io::{BufReader, Cursor};

use rusqlite::Connection;
use silman_db::import::{SourceInfo, SourceKind};
use silman_db::tactics::{
    self, elo_update, verify_move, MotifWeight, MoveVerdict, PuzzleImportOptions,
};

const FIXTURE: &str = "../../testdata/fixtures/puzzles_sample.csv";

fn source() -> SourceInfo {
    SourceInfo {
        name: "lichess-puzzles".into(),
        origin: "https://database.lichess.org/#puzzles".into(),
        license: "CC0-1.0".into(),
        kind: SourceKind::Other,
    }
}

fn fixture_db(opts: &PuzzleImportOptions) -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let reader = BufReader::new(File::open(FIXTURE).unwrap());
    tactics::import_puzzles_csv(&conn, &source(), reader, opts).unwrap();
    (dir, conn)
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[test]
fn import_fixture_counts_provenance_and_idempotence() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());

    // 500 fixture rows, none malformed, none filtered.
    assert_eq!(tactics::puzzle_count(&conn).unwrap(), 500);

    // Provenance row, exactly like other datasets.
    let (name, license): (String, String) = conn
        .query_row(
            "SELECT s.name, s.license FROM sources s
             JOIN puzzles p ON p.source_id = s.id
             GROUP BY s.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(name, "lichess-puzzles");
    assert_eq!(license, "CC0-1.0");

    // First fixture row spot-check (biPdd, rating 1627, 6-move line).
    let id: i64 = conn
        .query_row(
            "SELECT id FROM puzzles WHERE lichess_id = 'biPdd'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let p = tactics::load_puzzle(&conn, id).unwrap();
    assert_eq!(p.rating, 1627);
    assert_eq!(p.moves.len(), 6);
    assert_eq!(p.moves[0], "h1g1");
    assert!(p.themes.contains(&"endgame".to_string()));

    // Theme counts maintained at import time (awk ground truth).
    let themes = tactics::theme_list(&conn).unwrap();
    let count_of = |t: &str| themes.iter().find(|c| c.theme == t).map(|c| c.puzzles);
    assert_eq!(count_of("hangingPiece"), Some(12));
    assert_eq!(count_of("fork"), Some(58));
    assert_eq!(count_of("mate"), Some(158));
    assert_eq!(count_of("endgame"), Some(261));

    // Re-import is idempotent: everything is a duplicate, counts unchanged.
    let reader = BufReader::new(File::open(FIXTURE).unwrap());
    let st = tactics::import_puzzles_csv(&conn, &source(), reader, &PuzzleImportOptions::default())
        .unwrap();
    assert_eq!(st.imported, 0);
    assert_eq!(st.duplicates_skipped, 500);
    assert_eq!(tactics::puzzle_count(&conn).unwrap(), 500);
    let themes = tactics::theme_list(&conn).unwrap();
    assert_eq!(
        themes.iter().find(|c| c.theme == "fork").unwrap().puzzles,
        58,
        "duplicate import must not double theme counts"
    );
}

#[test]
fn import_filters_and_malformed_rows() {
    // min_popularity: 290 of the 500 fixture rows have Popularity >= 90.
    let (_dir, conn) = fixture_db(&PuzzleImportOptions {
        min_popularity: Some(90),
        max_rows: None,
    });
    assert_eq!(tactics::puzzle_count(&conn).unwrap(), 290);

    // max_rows stops the stream after N imported puzzles.
    let (_dir2, conn2) = fixture_db(&PuzzleImportOptions {
        min_popularity: None,
        max_rows: Some(100),
    });
    assert_eq!(tactics::puzzle_count(&conn2).unwrap(), 100);

    // Malformed rows are counted and skipped, not fatal.
    let dir = tempfile::tempdir().unwrap();
    let conn3 = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let csv =
        "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags\n\
               ok001,8/8/8/8/8/1k6/2q5/K7 b - - 0 1,c2b2,600,80,95,10,mate mateIn1 oneMove,url,\n\
               bad-rating,8/8/8/8/8/8/8/K6k w - - 0 1,a1a2,notanumber,80,95,10,mate,url,\n\
               short-row,onlytwo\n";
    let st = tactics::import_puzzles_csv(
        &conn3,
        &source(),
        Cursor::new(csv),
        &PuzzleImportOptions::default(),
    )
    .unwrap();
    assert_eq!(st.imported, 1);
    assert_eq!(st.malformed, 2);
}

// ---------------------------------------------------------------------------
// Rated selection
// ---------------------------------------------------------------------------

#[test]
fn rated_selection_stays_in_band_and_expands_when_starved() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());

    // 63 fixture puzzles sit in 1400..=1600: the first band must suffice.
    for seed in 0..20 {
        let p = tactics::next_rated(&conn, 1500, seed).unwrap().unwrap();
        assert!(
            (1400..=1600).contains(&p.rating),
            "seed {seed}: rating {} outside ±100 band",
            p.rating
        );
    }

    // Mark every puzzle in the band solved: selection must expand beyond
    // ±100 rather than starve (fixture max rating is 2785).
    conn.execute(
        "INSERT INTO puzzle_attempts (puzzle_id, solved, time_ms, rating_at_attempt)
         SELECT id, 1, 1000, 1500.0 FROM puzzles WHERE rating BETWEEN 1400 AND 1600",
        [],
    )
    .unwrap();
    let p = tactics::next_rated(&conn, 1500, 7).unwrap().unwrap();
    assert!(
        !(1400..=1600).contains(&p.rating),
        "solved puzzles must be excluded"
    );
    assert!((500..=2500).contains(&p.rating), "stays within max band");

    // A target far above every fixture puzzle: nothing within ±1000.
    assert!(tactics::next_rated(&conn, 5000, 1).unwrap().is_none());
}

#[test]
fn motif_filtered_selection_respects_theme() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    for seed in 0..10 {
        let p = tactics::next_by_theme(&conn, 1500, "fork", seed)
            .unwrap()
            .unwrap();
        assert!(p.themes.iter().any(|t| t == "fork"), "seed {seed}");
    }
    // A tag absent from the fixture set serves nothing.
    assert!(tactics::next_by_theme(&conn, 1500, "noSuchTheme", 0)
        .unwrap()
        .is_none());
}

#[test]
fn speed_drill_serves_easy_puzzles() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    for seed in 0..10 {
        let p = tactics::next_speed(&conn, 1800, seed).unwrap().unwrap();
        assert!(
            p.rating <= 1500,
            "seed {seed}: speed puzzle rated {} above user-300",
            p.rating
        );
    }
}

// ---------------------------------------------------------------------------
// Weakness-weighted selection
// ---------------------------------------------------------------------------

/// Synthetic profile mirroring the maintainer's real matrix shape:
/// Undefended allowed dominates (1318), everything else quiet.
fn synthetic_weights() -> Vec<MotifWeight> {
    vec![
        MotifWeight {
            kind: "Undefended".into(),
            allowed: 1318,
            missed: 200,
        },
        MotifWeight {
            kind: "WeakKing".into(),
            allowed: 40,
            missed: 10,
        },
        MotifWeight {
            kind: "TrappedPiece".into(),
            allowed: 12,
            missed: 3,
        },
    ]
}

fn undefended_mapped(p: &tactics::PuzzleRow) -> bool {
    p.themes.iter().any(|t| {
        matches!(
            t.as_str(),
            "hangingPiece" | "fork" | "discoveredAttack" | "skewer"
        )
    })
}

#[test]
fn weakness_weighting_measurably_shifts_the_distribution() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    let weights = synthetic_weights();
    const PICKS: u64 = 300;

    // Baseline: unweighted rated selection over the same database.
    let mut baseline_hits = 0u32;
    for seed in 0..PICKS {
        let p = tactics::next_rated(&conn, 1500, seed).unwrap().unwrap();
        if undefended_mapped(&p) {
            baseline_hits += 1;
        }
    }

    // Weighted: same seeds, same target.
    let mut weighted_hits = 0u32;
    let mut explained = 0u32;
    for seed in 0..PICKS {
        let c = tactics::next_weakness_weighted(&conn, 1500, &weights, seed)
            .unwrap()
            .unwrap();
        if undefended_mapped(&c.puzzle) {
            weighted_hits += 1;
            // The differentiator: the choice must be explainable.
            assert_eq!(c.motif.as_deref(), Some("Undefended"), "seed {seed}");
            assert_eq!(c.allowed, 1318);
            assert!(c.reason.contains("loose-piece"), "reason: {}", c.reason);
            assert!(!c.matched_themes.is_empty());
            assert!(c.weight > 1.0);
            explained += 1;
        }
    }

    let b = baseline_hits as f64 / PICKS as f64;
    let w = weighted_hits as f64 / PICKS as f64;
    // Visible under --nocapture; the shift is the product differentiator.
    println!(
        "weakness weighting: Undefended-mapped picks {baseline_hits}/{PICKS} ({:.1}%) unweighted \
         -> {weighted_hits}/{PICKS} ({:.1}%) weighted",
        b * 100.0,
        w * 100.0
    );
    assert!(
        w > b * 1.5,
        "weighting must measurably shift the distribution: \
         baseline {baseline_hits}/{PICKS} ({b:.3}), weighted {weighted_hits}/{PICKS} ({w:.3})"
    );
    assert!(explained > 0);

    // Determinism: identical inputs, identical choice.
    let a = tactics::next_weakness_weighted(&conn, 1500, &weights, 42)
        .unwrap()
        .unwrap();
    let b2 = tactics::next_weakness_weighted(&conn, 1500, &weights, 42)
        .unwrap()
        .unwrap();
    assert_eq!(a.puzzle.id, b2.puzzle.id);
}

#[test]
fn weakness_weighting_without_profile_falls_back_gracefully() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    let c = tactics::next_weakness_weighted(&conn, 1500, &[], 3)
        .unwrap()
        .unwrap();
    assert!(c.motif.is_none());
    assert!((c.weight - 1.0).abs() < 1e-9);
    assert!(c.reason.contains("no profiled weaknesses"));
}

// ---------------------------------------------------------------------------
// Rating math + attempt history
// ---------------------------------------------------------------------------

#[test]
fn elo_update_hand_computed_values() {
    // Even match, provisional K=40: expected 0.5, so ±20.
    assert!((elo_update(1500.0, 0, 1500.0, true) - 1520.0).abs() < 1e-9);
    assert!((elo_update(1500.0, 0, 1500.0, false) - 1480.0).abs() < 1e-9);
    // vs +200 puzzle: expected = 1/(1+10^0.5) = 0.240253...
    // solve: 1500 + 40*(1-0.240253) = 1530.3899; fail: 1500 - 40*0.240253.
    assert!((elo_update(1500.0, 0, 1700.0, true) - 1530.389877395476).abs() < 1e-6);
    assert!((elo_update(1500.0, 0, 1700.0, false) - 1490.389877395476).abs() < 1e-6);
    // Established K=20 after 30 rated attempts.
    assert!((elo_update(1500.0, 30, 1500.0, true) - 1510.0).abs() < 1e-9);
    // Clamps: losing to a WEAKER puzzle costs the most (expected ~0.65),
    // 505 - 20*0.646 < 500 → floor; the mirror case hits the ceiling.
    assert_eq!(elo_update(505.0, 100, 400.0, false), 500.0);
    assert_eq!(elo_update(3195.0, 100, 3300.0, true), 3200.0);
}

#[test]
fn attempt_history_and_rating_ledger() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    let start = tactics::tactics_rating(&conn).unwrap();
    assert_eq!(start.rating, 1500.0);
    assert_eq!(start.attempts, 0);

    let p = tactics::next_rated(&conn, 1500, 11).unwrap().unwrap();
    let out = tactics::record_attempt(&conn, p.id, true, 12_345, "rated", None).unwrap();
    assert_eq!(out.rating_before, 1500.0);
    let expect = elo_update(1500.0, 0, p.rating as f64, true);
    assert!((out.rating_after - expect).abs() < 1e-9);
    assert_eq!(out.attempts, 1);

    // History row, with the PRE-update rating.
    let (solved, time_ms, rating_at, mode): (i64, i64, f64, String) = conn
        .query_row(
            "SELECT solved, time_ms, rating_at_attempt, mode
             FROM puzzle_attempts WHERE puzzle_id = ?1",
            [p.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((solved, time_ms, mode.as_str()), (1, 12_345, "rated"));
    assert_eq!(rating_at, 1500.0);

    // Speed attempts record history but never move the rating.
    let before = tactics::tactics_rating(&conn).unwrap();
    let out = tactics::record_attempt(&conn, p.id, false, 900, "speed", None).unwrap();
    assert_eq!(out.rating_before, out.rating_after);
    let after = tactics::tactics_rating(&conn).unwrap();
    assert_eq!(before.rating, after.rating);
    assert_eq!(
        before.attempts, after.attempts,
        "rated-attempt counter untouched"
    );
    let history: i64 = conn
        .query_row("SELECT COUNT(*) FROM puzzle_attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(history, 2);

    // Unknown modes are rejected.
    assert!(tactics::record_attempt(&conn, p.id, true, 1, "blitz", None).is_err());
}

// ---------------------------------------------------------------------------
// Woodpecker cycles
// ---------------------------------------------------------------------------

#[test]
fn woodpecker_cycle_stats_math() {
    let (_dir, conn) = fixture_db(&PuzzleImportOptions::default());
    let set_id = tactics::create_woodpecker_set(&conn, "daily-20", 20, 1500, 99).unwrap();
    let ids = tactics::woodpecker_set_puzzles(&conn, set_id).unwrap();
    assert_eq!(ids.len(), 20);
    let sets = tactics::woodpecker_sets(&conn).unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!((sets[0].size, sets[0].cycles), (20, 0));

    // Cycle 1: 20 attempts, 15 solved, 8s each. Cycle 2: 20/19, 3s each.
    let c1 = tactics::start_woodpecker_cycle(&conn, set_id).unwrap();
    for (i, &pid) in ids.iter().enumerate() {
        tactics::record_attempt(&conn, pid, i < 15, 8_000, "woodpecker", Some(c1)).unwrap();
    }
    tactics::finish_woodpecker_cycle(&conn, c1).unwrap();
    let c2 = tactics::start_woodpecker_cycle(&conn, set_id).unwrap();
    for (i, &pid) in ids.iter().enumerate() {
        tactics::record_attempt(&conn, pid, i < 19, 3_000, "woodpecker", Some(c2)).unwrap();
    }
    tactics::finish_woodpecker_cycle(&conn, c2).unwrap();

    let stats = tactics::woodpecker_cycle_stats(&conn, set_id).unwrap();
    assert_eq!(stats.len(), 2);
    assert_eq!(
        (stats[0].cycle_no, stats[0].attempts, stats[0].solved),
        (1, 20, 15)
    );
    assert_eq!(stats[0].accuracy_pct, 75.0);
    assert_eq!(stats[0].total_time_ms, 160_000);
    assert_eq!(stats[0].avg_time_ms, 8_000);
    assert!(stats[0].finished_at.is_some());
    assert_eq!(
        (stats[1].cycle_no, stats[1].attempts, stats[1].solved),
        (2, 20, 19)
    );
    assert_eq!(stats[1].accuracy_pct, 95.0);
    assert_eq!(stats[1].avg_time_ms, 3_000);

    // Woodpecker attempts never moved the rating.
    let r = tactics::tactics_rating(&conn).unwrap();
    assert_eq!(r.rating, 1500.0);
    assert_eq!(r.attempts, 0);

    // Cycle numbering is per set and finishing twice fails.
    assert!(tactics::finish_woodpecker_cycle(&conn, c1).is_err());
    assert!(tactics::start_woodpecker_cycle(&conn, 999).is_err());

    // Determinism: same seed, same set contents.
    let again = tactics::create_woodpecker_set(&conn, "daily-20-b", 20, 1500, 99).unwrap();
    assert_eq!(
        tactics::woodpecker_set_puzzles(&conn, again).unwrap(),
        ids,
        "same seed must draw the same set"
    );
}

// ---------------------------------------------------------------------------
// Solve verification (exact match + alternate mates + castling forms)
// ---------------------------------------------------------------------------

#[test]
fn verify_move_exact_wrong_and_alt_mate() {
    // Fixture puzzle gEacS: Black to solve after the setup move. The
    // solver's first move is f3g5 from the puzzle FEN's successor; here we
    // check directly against the puzzle's own stored position + line.
    let fen = "6k1/p3rpp1/2p2r2/8/4p1q1/P1N1PnP1/1P2RPK1/3Q3R b - - 1 30";
    assert_eq!(
        verify_move(fen, "f3g5", "f3g5").unwrap(),
        MoveVerdict::Correct
    );
    assert_eq!(
        verify_move(fen, "f3g5", "f3d2").unwrap(),
        MoveVerdict::Wrong
    );
    // Illegal and garbage input is Wrong, not an error.
    assert_eq!(
        verify_move(fen, "f3g5", "a7a5").unwrap(),
        MoveVerdict::Wrong
    );
    assert_eq!(
        verify_move(fen, "f3g5", "zz99").unwrap(),
        MoveVerdict::Wrong
    );

    // Alternate mate: both Ra8# and Qb8# mate; the stored answer is Ra8#.
    let two_mates = "6k1/5ppp/8/8/8/8/8/RQ4K1 w - - 0 1";
    assert_eq!(
        verify_move(two_mates, "a1a8", "a1a8").unwrap(),
        MoveVerdict::Correct
    );
    assert_eq!(
        verify_move(two_mates, "a1a8", "b1b8").unwrap(),
        MoveVerdict::CorrectAltMate
    );
    // A legal non-mating queen move is just wrong.
    assert_eq!(
        verify_move(two_mates, "a1a8", "b1b7").unwrap(),
        MoveVerdict::Wrong
    );

    // A checking-but-not-mating alternative stays Wrong (escape square h7
    // after h7h5 would be a different position; here Qb3+ is check only).
    let check_not_mate = "6k1/5p2/8/8/8/8/8/RQ4K1 w - - 0 1";
    assert_eq!(
        verify_move(check_not_mate, "a1a8", "b1b3").unwrap(),
        MoveVerdict::Wrong
    );
}

#[test]
fn verify_move_accepts_both_castling_forms_and_promotions() {
    let castle = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    // Lichess UCI ("e1g1") vs cozy-chess king-onto-rook ("e1h1"): all four
    // combinations of stored/played forms must agree.
    for expected in ["e1g1", "e1h1"] {
        for played in ["e1g1", "e1h1"] {
            assert_eq!(
                verify_move(castle, expected, played).unwrap(),
                MoveVerdict::Correct,
                "expected {expected}, played {played}"
            );
        }
    }
    assert_eq!(
        verify_move(castle, "e1g1", "e1c1").unwrap(),
        MoveVerdict::Wrong,
        "long castle is not short castle"
    );

    // Promotion: piece letter must match exactly (underpromotion counts).
    let promo = "8/4P1k1/8/8/8/8/8/K7 w - - 0 1";
    assert_eq!(
        verify_move(promo, "e7e8q", "e7e8q").unwrap(),
        MoveVerdict::Correct
    );
    assert_eq!(
        verify_move(promo, "e7e8n", "e7e8q").unwrap(),
        MoveVerdict::Wrong
    );
}
