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
use silman_db::endgame::{self, curriculum, Drill, DrillSession, Goal, Verdict};
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
    let mut first_rows = None;
    for (i, uci) in KP_WIN_SCRIPT.iter().enumerate() {
        assert!(s.outcome().is_none(), "drill ended early at move {i}");
        let report = s
            .user_move(uci, None)
            .unwrap_or_else(|e| panic!("scripted move {uci} rejected: {e}"));
        if i == 0 {
            first_rows = Some(report.rows.clone());
        }
    }
    let outcome = s.outcome().expect("drill finished");
    assert!(outcome.solved, "expected a win, got {:?}", outcome.detail);
    assert_eq!(outcome.detail, "Checkmate!");
    assert_eq!(s.user_moves(), KP_WIN_SCRIPT.len() as u32);
    assert_eq!(s.opponent_kind(), "heuristic");
    assert_eq!(s.verification_kind(), "terminal");

    // Feedback rows without tablebase files: user moves are honestly
    // `unverified`, opponent replies carry the `engine` label.
    let first = first_rows.expect("first step captured");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].verdict, Verdict::Unverified);
    assert_eq!(first[0].san, "e4", "SAN of the user's pawn push e3e4");
    assert_eq!(first[1].verdict, Verdict::Engine);
    assert!(!first[1].san.is_empty());
    // Terminals are ground truth even without tables: the mating move is
    // graded `winning`. 13 user moves, 12 opponent replies.
    let rows = s.verdict_rows();
    assert_eq!(rows.len(), 25);
    assert_eq!(rows.last().unwrap().verdict, Verdict::Winning);
    assert_eq!(rows.last().unwrap().note, "Checkmate!");
    assert_eq!(rows.last().unwrap().index, 25, "rows are 1-based, ordered");
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

/// Round-2 item 4: verdict rows graded purely from tablebase probes.
/// A fixture 3-man KQvK drill in which all three user verdicts are
/// reachable in one move: the DTZ-fastest move → `winning`, a
/// win-preserving shuffle with a worse DTZ → `slower` (cost stated), and
/// hanging the queen → `throws`.
#[test]
fn verdict_rows_grade_against_tablebase_truth() {
    let mut tb = require_tb!();
    let fixture = Drill {
        id: "test-kq-verdicts".into(),
        tier: "test".into(),
        title: "KQvK verdict fixture".into(),
        concept: "queen_mate".into(),
        material: "KQvK".into(),
        // Qg2/Ke1 vs Ke6: Qd5+?? Kxd5 hangs the queen (draw); quiet
        // queen moves keep the win at varying DTZ pace.
        fen: "8/8/4k3/8/8/8/6Q1/4K3 w - - 0 1".into(),
        goal: Goal::Win,
        instruction: "test fixture".into(),
    };
    let start = Board::from_fen(&fixture.fen, false).unwrap();

    // Classify every legal move by probing, from the user's perspective.
    let score_of = |wdl: Wdl| -> i8 {
        match wdl {
            Wdl::Loss => -2,
            Wdl::BlessedLoss => -1,
            Wdl::Draw => 0,
            Wdl::CursedWin => 1,
            Wdl::Win => 2,
        }
    };
    let pre_dtz = match tb.probe_root_board(&start).unwrap() {
        RootProbe::Move(m) => {
            assert_eq!(m.wdl, Wdl::Win, "fixture must be winning");
            m.dtz
        }
        other => panic!("fixture is not terminal, got {other:?}"),
    };
    let mut moves: Vec<Move> = Vec::new();
    start.generate_moves(|ml| {
        moves.extend(ml);
        false
    });
    let mut fastest = None; // (uci, cost == 0)
    let mut slow = None; // win kept, cost > 0
    let mut throwing = None; // result flipped
    for mv in moves {
        let mut b = start.clone();
        b.play(mv);
        // Opponent to move: negate for the user's perspective.
        let (user_score, dtz) = match tb.probe_root_board(&b).unwrap() {
            RootProbe::Checkmate => (2, 0),
            RootProbe::Stalemate => (0, 0),
            RootProbe::Move(m) => (-score_of(m.wdl), m.dtz),
        };
        let cost = dtz as i64 + 1 - pre_dtz as i64;
        if user_score >= 2 && cost <= 0 && fastest.is_none() {
            fastest = Some(mv.to_string());
        } else if user_score >= 2 && cost > 0 && slow.is_none() {
            slow = Some((mv.to_string(), cost));
        } else if user_score < 2 && throwing.is_none() {
            throwing = Some(mv.to_string());
        }
    }
    let fastest = fastest.expect("a DTZ-optimal move exists");
    let (slow, slow_cost) = slow.expect("a slower-but-winning move exists");
    let throwing = throwing.expect("a throwing move exists (Qd5+??)");

    // Best move → winning, and the opponent's reply is an `engine` row.
    let mut s = DrillSession::new(&fixture).unwrap();
    let report = s.user_move(&fastest, Some(&mut tb)).unwrap();
    assert_eq!(report.rows.len(), 2);
    assert_eq!(report.rows[0].verdict, Verdict::Winning);
    assert_eq!(report.rows[0].dtz_cost, None);
    assert_eq!(report.rows[1].verdict, Verdict::Engine);
    assert_eq!(s.verdict_rows(), &report.rows[..], "session accumulates");

    // Worse-but-winning → slower, with the DTZ cost stated.
    let mut s = DrillSession::new(&fixture).unwrap();
    let report = s.user_move(&slow, Some(&mut tb)).unwrap();
    assert_eq!(report.rows[0].verdict, Verdict::Slower);
    assert_eq!(report.rows[0].dtz_cost, Some(slow_cost as u32));
    assert!(
        report.rows[0].note.contains("longer"),
        "note states the cost: {}",
        report.rows[0].note
    );
    assert!(report.outcome.is_none(), "slower does not end the drill");

    // Drawing move → throws, and the drill fails on the spot.
    let mut s = DrillSession::new(&fixture).unwrap();
    let report = s.user_move(&throwing, Some(&mut tb)).unwrap();
    assert_eq!(report.rows.len(), 1, "no opponent reply after a throw");
    assert_eq!(report.rows[0].verdict, Verdict::Throws);
    let outcome = report.outcome.expect("throw ends the drill");
    assert!(!outcome.solved);

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
