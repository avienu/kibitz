//! Endgame trainer tests: curriculum integrity, the drill state machine
//! against the deterministic heuristic opponent, tablebase-gated behavior
//! (result-flip policing, optimal opponent replies, curriculum ground
//! truth), and attempt/mastery recording.
//!
//! Tablebase-gated tests follow silman-tb's own pattern: they skip
//! gracefully (with a note on stderr) when testdata/syzygy is absent — run
//! `bash scripts/fetch_syzygy_test_files.sh` to enable them.
//!
//! The scripted lines were derived offline by playing Syzygy-optimal user
//! moves against the (fully deterministic) heuristic opponent; the ground
//! truth of every curriculum FEN was verified against Syzygy tablebases
//! (lichess.ovh API, 2026-07-26).
//!
//! Engine-off (CLAUDE.md #6): the drill engine must never spawn Stockfish;
//! each behavioral test asserts the spawn counter stayed at zero.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use cozy_chess::{Board, Color, Move, Piece};
use silman_db::endgame::{self, curriculum, DrillSession, Goal};
use silman_tb::{RootProbe, Tablebase, Wdl};

// ---------------------------------------------------------------------------
// Shared tablebase (Fathom is process-global), with graceful skip
// ---------------------------------------------------------------------------

fn tb_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/syzygy")
}

fn tablebase() -> Option<MutexGuard<'static, Tablebase>> {
    static TB: OnceLock<Option<Mutex<Tablebase>>> = OnceLock::new();
    TB.get_or_init(|| {
        let dir = tb_dir();
        let has_tables = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().is_some_and(|ext| ext == "rtbw"))
            })
            .unwrap_or(false);
        if !has_tables {
            eprintln!(
                "SKIPPING endgame tablebase tests: no .rtbw files in {} \
                 (run scripts/fetch_syzygy_test_files.sh)",
                dir.display()
            );
            return None;
        }
        Some(Mutex::new(
            Tablebase::init(&dir).expect("Tablebase::init on testdata/syzygy"),
        ))
    })
    .as_ref()
    .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

macro_rules! require_tb {
    () => {
        match tablebase() {
            Some(tb) => tb,
            None => return, // graceful skip; reason already printed
        }
    };
}

// ---------------------------------------------------------------------------
// Curriculum integrity
// ---------------------------------------------------------------------------

/// "KQRBNP-order white pieces" + 'v' + same for black, e.g. "KRPvKR".
fn material_sig(board: &Board) -> String {
    let mut s = String::new();
    for color in [Color::White, Color::Black] {
        if color == Color::Black {
            s.push('v');
        }
        for (piece, letter) in [
            (Piece::King, 'K'),
            (Piece::Queen, 'Q'),
            (Piece::Rook, 'R'),
            (Piece::Bishop, 'B'),
            (Piece::Knight, 'N'),
            (Piece::Pawn, 'P'),
        ] {
            let n = (board.pieces(piece) & board.colors(color)).len();
            for _ in 0..n {
                s.push(letter);
            }
        }
    }
    s
}

#[test]
fn curriculum_structure_tiers_and_required_concepts() {
    let c = curriculum();
    assert_eq!(c.version, 1);
    assert!(
        (25..=40).contains(&c.drills.len()),
        "expected 25-40 drills, got {}",
        c.drills.len()
    );
    assert_eq!(c.tiers.len(), 3);
    for tier in &c.tiers {
        assert!(!tier.rating_band.is_empty() && !tier.summary.is_empty());
    }

    // Unique ids; every drill's tier exists; every tier is non-empty.
    let mut ids: Vec<&str> = c.drills.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), c.drills.len(), "duplicate drill ids");
    for d in &c.drills {
        assert!(
            c.tiers.iter().any(|t| t.id == d.tier),
            "drill {} references unknown tier {}",
            d.id,
            d.tier
        );
        assert!(!d.title.is_empty() && !d.instruction.is_empty());
    }
    for tier in &c.tiers {
        assert!(c.drills.iter().any(|d| d.tier == tier.id));
    }

    // Required concept coverage (public-domain theory only).
    for concept in [
        "queen_mate",
        "rook_mate",
        "square_of_pawn",
        "key_squares",
        "opposition",
        "rook_pawn_draw",
        "spare_tempo",
        "queen_vs_pawn",
        "lucena",
        "philidor",
        "back_rank_defense",
        "vancura",
        "rook_cutoff",
        "triangulation",
        "wrong_bishop",
    ] {
        assert!(
            c.drills.iter().any(|d| d.concept == concept),
            "missing required concept {concept}"
        );
    }
    // Q vs P must cover both the winning and the drawing (a/c-pawn) cases.
    let qvp_goals: Vec<Goal> = c
        .drills
        .iter()
        .filter(|d| d.concept == "queen_vs_pawn")
        .map(|d| d.goal)
        .collect();
    assert!(qvp_goals.contains(&Goal::Win) && qvp_goals.contains(&Goal::Draw));
    let qvp_draws = qvp_goals.iter().filter(|g| **g == Goal::Draw).count();
    assert!(qvp_draws >= 2, "need both the a-pawn and c-pawn draw cases");
    // Several K+P vs K key-square/opposition positions.
    let kpvk = c
        .drills
        .iter()
        .filter(|d| d.material == "KPvK" && d.concept != "square_of_pawn")
        .count();
    assert!(kpvk >= 4, "need several K+P vs K positions, got {kpvk}");
}

#[test]
fn every_fen_is_legal_with_the_stated_material() {
    // Expected material per concept — e.g. a Lucena MUST be exactly
    // K+R+P vs K+R (from the user's = White's side in our positions).
    fn expected_material(concept: &str) -> &'static str {
        match concept {
            "queen_mate" => "KQvK",
            "rook_mate" => "KRvK",
            "square_of_pawn" | "key_squares" | "opposition" | "spare_tempo" | "rook_pawn_draw" => {
                "KPvK"
            }
            "queen_vs_pawn" => "KQvKP",
            "lucena" | "philidor" | "back_rank_defense" | "vancura" | "rook_cutoff" => "KRPvKR",
            "triangulation" => "KPvKP",
            "wrong_bishop" => "KBPvK",
            other => panic!("unexpected concept {other}"),
        }
    }
    for d in &curriculum().drills {
        let board = Board::from_fen(&d.fen, false)
            .unwrap_or_else(|e| panic!("drill {}: illegal FEN {:?}: {e}", d.id, d.fen));
        let sig = material_sig(&board);
        assert_eq!(sig, d.material, "drill {}: material field vs FEN", d.id);
        assert_eq!(
            sig,
            expected_material(&d.concept),
            "drill {}: material does not match concept {}",
            d.id,
            d.concept
        );
        // Every drill must start mid-game (the user has a move to find).
        let mut any = false;
        board.generate_moves(|_| {
            any = true;
            true
        });
        assert!(any, "drill {} starts in a terminal position", d.id);
    }
}

// ---------------------------------------------------------------------------
// Drill state machine (heuristic opponent, no tablebase)
// ---------------------------------------------------------------------------

/// Derived offline (see module docs): Syzygy-optimal user moves for the
/// "kp-king-in-front" win drill against the deterministic heuristic. The
/// heuristic's replies are deterministic, so this exact line replays.
const KP_WIN_SCRIPT: &[&str] = &[
    "e3e4", "e5f6", "e4e5", "e5e6", "e6e7", "f6f7", "e7e8q", "e8a4", "f7e6", "e6d6", "a4b3",
    "d6c6", "b3b7",
];

#[test]
fn scripted_kp_win_reaches_mate_without_tablebase() {
    let drill = endgame::drill("kp-king-in-front").expect("drill exists");
    let mut s = DrillSession::new(drill).unwrap();
    for (i, uci) in KP_WIN_SCRIPT.iter().enumerate() {
        assert!(s.outcome().is_none(), "drill ended early at move {i}");
        s.user_move(uci, None)
            .unwrap_or_else(|e| panic!("scripted move {uci} rejected: {e}"));
    }
    let outcome = s.outcome().expect("drill finished");
    assert!(outcome.solved, "expected a win, got {:?}", outcome.detail);
    assert_eq!(outcome.detail, "Checkmate!");
    assert_eq!(s.user_moves(), KP_WIN_SCRIPT.len() as u32);
    assert_eq!(s.opponent_kind(), "heuristic");
    assert_eq!(s.verification_kind(), "terminal");
    assert_eq!(silman_db::engine::spawn_count(), 0, "engine-off violated");
}

#[test]
fn scripted_wrong_bishop_draw_holds_without_tablebase() {
    // The defender's whole plan is a policy, not a move list: sit in the
    // corner the wrong bishop cannot control (h8/g8), whatever White tries.
    let drill = endgame::drill("wrong-bishop").expect("drill exists");
    assert_eq!(drill.goal, Goal::Draw);
    let mut s = DrillSession::new(drill).unwrap();
    for _ in 0..200 {
        if s.outcome().is_some() {
            break;
        }
        let mut moves: Vec<Move> = Vec::new();
        s.board().generate_moves(|ml| {
            moves.extend(ml);
            false
        });
        let pick = ["h8", "g8", "g7", "h7"]
            .iter()
            .find_map(|dest| moves.iter().find(|m| m.to.to_string() == *dest))
            .or_else(|| moves.iter().min_by_key(|m| m.to_string()))
            .copied()
            .expect("defender has a legal move");
        s.user_move(&pick.to_string(), None).unwrap();
    }
    let outcome = s.outcome().expect("drill should reach a terminal draw");
    assert!(
        outcome.solved,
        "corner defense must hold the draw, got {:?}",
        outcome.detail
    );
    assert_eq!(s.opponent_kind(), "heuristic");
    assert_eq!(silman_db::engine::spawn_count(), 0, "engine-off violated");
}

// ---------------------------------------------------------------------------
// Tablebase-gated behavior (skips without testdata/syzygy)
// ---------------------------------------------------------------------------

#[test]
fn tb_flips_fail_the_drill_immediately() {
    let mut tb = require_tb!();
    // "square-rule-race" is won by 1.a5 (defender outside the square);
    // 1.Kb2?? lets the king step in — a tablebase-verified draw, and the
    // drill must fail on the spot rather than play on to a dead draw.
    let drill = endgame::drill("square-rule-race").expect("drill exists");
    let mut s = DrillSession::new(drill).unwrap();
    let report = s.user_move("a1b2", Some(&mut tb)).unwrap();
    let outcome = report.outcome.expect("flip must end the drill");
    assert!(!outcome.solved);
    assert!(
        outcome.detail.contains("throws away the win"),
        "unexpected detail: {}",
        outcome.detail
    );
    assert!(report.opponent.is_none(), "no reply after a failing move");
    assert_eq!(s.verification_kind(), "tablebase");
    assert_eq!(silman_db::engine::spawn_count(), 0, "engine-off violated");
}

#[test]
fn tb_opponent_replies_preserve_the_theoretical_result() {
    let mut tb = require_tb!();
    // 3-man drill: the opponent must answer from the tablebase, and its
    // reply must leave the position still theoretically won for the user.
    let drill = endgame::drill("kq-mate-edge").expect("drill exists");
    let mut s = DrillSession::new(drill).unwrap();
    let report = s.user_move("a1a4", Some(&mut tb)).unwrap();
    let opp = report.opponent.expect("game continues");
    assert_eq!(format!("{:?}", opp.source), "Tablebase");
    assert!(report.outcome.is_none());
    match tb.probe_root_board(s.board()).unwrap() {
        RootProbe::Move(m) => assert_eq!(m.wdl, Wdl::Win, "user must still be winning"),
        other => panic!("expected an ongoing position, got {other:?}"),
    }

    // And the whole drill is winnable to mate with optimal user moves:
    // opponent replies and flip-policing both come from the tablebase.
    for _ in 0..60 {
        if s.outcome().is_some() {
            break;
        }
        let RootProbe::Move(m) = tb.probe_root_board(s.board()).unwrap() else {
            panic!("user to move but position is terminal");
        };
        let mv = Move {
            from: m.from,
            to: m.to,
            promotion: m.promotion,
        };
        s.user_move(&mv.to_string(), Some(&mut tb)).unwrap();
    }
    let outcome = s.outcome().expect("mate within 60 moves");
    assert!(outcome.solved, "got {:?}", outcome.detail);
    assert_eq!(s.opponent_kind(), "tablebase");
    assert_eq!(s.verification_kind(), "tablebase");
    assert_eq!(silman_db::engine::spawn_count(), 0, "engine-off violated");
}

#[test]
fn three_man_curriculum_fens_match_their_goal_in_the_tablebase() {
    let tb = require_tb!();
    let mut checked = 0;
    for d in &curriculum().drills {
        let board = Board::from_fen(&d.fen, false).unwrap();
        if board.occupied().len() > tb.largest() {
            continue; // beyond the downloaded test set
        }
        let wdl = tb.probe_board(&board).unwrap();
        let expected = match d.goal {
            Goal::Win => Wdl::Win,
            Goal::Draw => Wdl::Draw,
        };
        assert_eq!(wdl, expected, "drill {}: goal vs tablebase truth", d.id);
        checked += 1;
    }
    // The 3-man set covers all KPvK / KQvK / KRvK drills.
    assert!(checked >= 10, "only {checked} drills were 3-man");
}

// ---------------------------------------------------------------------------
// Attempts and mastery
// ---------------------------------------------------------------------------

#[test]
fn attempts_and_mastery_recording() {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let id = "lucena";

    // Failure first: attempt logged, streak stays zero.
    let p = endgame::record_attempt(&conn, id, false, 4, 30_000, "heuristic", "terminal").unwrap();
    assert_eq!(
        (p.attempts, p.solved, p.clean_streak, p.mastered),
        (1, 0, 0, false)
    );

    // Two clean completions in a row => mastered.
    let p = endgame::record_attempt(&conn, id, true, 12, 60_000, "heuristic", "terminal").unwrap();
    assert_eq!((p.attempts, p.clean_streak, p.mastered), (2, 1, false));
    let p = endgame::record_attempt(&conn, id, true, 11, 55_000, "mixed", "tablebase").unwrap();
    assert_eq!((p.attempts, p.clean_streak, p.mastered), (3, 2, true));

    // A later failure resets the streak but mastery persists.
    let p = endgame::record_attempt(&conn, id, false, 2, 9_000, "tablebase", "tablebase").unwrap();
    assert_eq!(
        (p.attempts, p.solved, p.clean_streak, p.mastered),
        (4, 2, 0, true)
    );

    // History rows carry the opponent/verification provenance.
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT solved, opponent, verification FROM endgame_attempts
                 WHERE drill_id = ?1 ORDER BY id",
            )
            .unwrap();
        let rows = stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0], (0, "heuristic".into(), "terminal".into()));
    assert_eq!(rows[3], (0, "tablebase".into(), "tablebase".into()));

    // progress_all mirrors the mastery table.
    let all = endgame::progress_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].mastered && all[0].drill_id == id);

    // Unknown drills are rejected.
    assert!(
        endgame::record_attempt(&conn, "no-such-drill", true, 1, 1, "none", "terminal").is_err()
    );
}

#[test]
fn resign_records_as_failure() {
    let drill = endgame::drill("philidor").expect("drill exists");
    let mut s = DrillSession::new(drill).unwrap();
    s.user_move("a6a1", None).unwrap(); // premature rook retreat, game on
    s.resign();
    let o = s.outcome().expect("resigned");
    assert!(!o.solved);
    assert_eq!(silman_db::engine::spawn_count(), 0, "engine-off violated");
}
