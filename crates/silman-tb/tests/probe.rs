//! Probe tests against real Syzygy files in testdata/syzygy/ (git-ignored).
//!
//! Run `bash scripts/fetch_syzygy_test_files.sh` (repo root) to download the
//! 3-man set (~26 KB). If the files are absent every test skips with a note
//! on stderr, so CI stays green without them.
//!
//! Ground truth for every FEN was cross-checked against the Lichess Syzygy
//! tablebase API (tablebase.lichess.ovh) on 2026-07-26.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use cozy_chess::{Board, Piece};
use silman_tb::{RootProbe, Tablebase, TbError, Wdl};

fn tb_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/syzygy")
}

/// One process-wide Tablebase (Fathom is global); None if testdata is absent.
/// The Mutex both serializes `probe_root` (&mut) and hands out shared access.
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
                "SKIPPING silman-tb probe tests: no .rtbw files in {} \
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

fn board(fen: &str) -> Board {
    Board::from_fen(fen, false).unwrap_or_else(|e| panic!("bad FEN {fen:?}: {e}"))
}

fn wdl(tb: &Tablebase, fen: &str) -> Result<Wdl, TbError> {
    tb.probe_board(&board(fen))
}

#[test]
fn largest_covers_at_least_three_men() {
    let tb = require_tb!();
    assert!(tb.largest() >= 3, "largest() = {}", tb.largest());
}

#[test]
fn kqvk_win_and_loss() {
    let tb = require_tb!();
    // Qa1 Ke1 vs Ke8. White to move mates (Lichess: win, DTZ 13).
    assert_eq!(
        wdl(&tb, "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1").unwrap(),
        Wdl::Win
    );
    // Same position, Black to move: Black is lost (Lichess: loss).
    assert_eq!(
        wdl(&tb, "4k3/8/8/8/8/8/8/Q3K3 b - - 0 1").unwrap(),
        Wdl::Loss
    );
    // Qc1 Kd1 vs Kd6 — second sample pair, same truth (Lichess verified).
    assert_eq!(
        wdl(&tb, "8/8/3k4/8/8/8/8/2QK4 w - - 0 1").unwrap(),
        Wdl::Win
    );
    assert_eq!(
        wdl(&tb, "8/8/3k4/8/8/8/8/2QK4 b - - 0 1").unwrap(),
        Wdl::Loss
    );
}

#[test]
fn krvk_win() {
    let tb = require_tb!();
    // Ra1 Ke1 vs Ke8, White to move (Lichess: win, DTZ 23).
    assert_eq!(
        wdl(&tb, "4k3/8/8/8/8/8/8/R3K3 w - - 0 1").unwrap(),
        Wdl::Win
    );
}

#[test]
fn kpvk_won_and_drawn() {
    let tb = require_tb!();
    // Ka1, Pa2 vs Kh8: rook pawn but the black king is in the wrong corner
    // and cannot reach a8 in time — win (Lichess: win, best a4).
    assert_eq!(wdl(&tb, "7k/8/8/8/8/8/P7/K7 w - - 0 1").unwrap(), Wdl::Win);
    // Ka1, Pa2 vs Ka8: classic rook-pawn draw — the defending king already
    // controls the promotion corner (Lichess: draw, either side to move).
    assert_eq!(wdl(&tb, "k7/8/8/8/8/8/P7/K7 w - - 0 1").unwrap(), Wdl::Draw);
    assert_eq!(wdl(&tb, "k7/8/8/8/8/8/P7/K7 b - - 0 1").unwrap(), Wdl::Draw);
}

#[test]
fn bare_kings_is_draw() {
    let tb = require_tb!();
    // KvK: insufficient material. Syzygy sets have no 2-man table; the
    // wrapper answers Draw directly (documented on Tablebase::probe_wdl).
    assert_eq!(
        wdl(&tb, "8/8/8/4k3/8/8/8/4K3 w - - 0 1").unwrap(),
        Wdl::Draw
    );
}

#[test]
fn castling_rights_are_rejected() {
    let tb = require_tb!();
    // KRvK but White still has queenside castling rights.
    let err = wdl(&tb, "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1").unwrap_err();
    assert!(matches!(err, TbError::CastlingRights), "got {err:?}");
}

#[test]
fn nonzero_rule50_is_rejected_for_wdl() {
    let tb = require_tb!();
    let err = wdl(&tb, "4k3/8/8/8/8/8/8/Q3K3 w - - 5 10").unwrap_err();
    assert!(matches!(err, TbError::NonzeroRule50(5)), "got {err:?}");
}

#[test]
fn six_pieces_reports_too_many() {
    let tb = require_tb!();
    // 6 men (KQQ vs Kpp) — beyond any 3-4-5 set, and far beyond the 3-man
    // test files, so this must fail cleanly before reaching C.
    let err = wdl(&tb, "4k3/pp6/8/8/8/8/8/QQ2K3 w - - 0 1").unwrap_err();
    match err {
        TbError::TooManyPieces { count, largest } => {
            assert_eq!(count, 6);
            assert_eq!(largest, tb.largest());
        }
        other => panic!("expected TooManyPieces, got {other:?}"),
    }
}

#[test]
fn probe_root_kqvk_suggests_winning_move() {
    let mut tb = require_tb!();
    let b = board("4k3/8/8/8/8/8/8/Q3K3 w - - 0 1");
    match tb.probe_root_board(&b).unwrap() {
        RootProbe::Move(m) => {
            assert_eq!(m.wdl, Wdl::Win);
            assert!(m.dtz >= 1, "dtz = {}", m.dtz);
            assert_eq!(m.promotion, None);
            assert!(!m.en_passant);
            // The suggested move must be legal in the position.
            let mv = cozy_chess::Move {
                from: m.from,
                to: m.to,
                promotion: None,
            };
            assert!(b.is_legal(mv), "illegal suggested move {mv}");
        }
        other => panic!("expected a move, got {other:?}"),
    }
}

#[test]
fn probe_root_kpvk_draw_and_promotion_paths() {
    let mut tb = require_tb!();
    // Drawn rook-pawn position: root probe must agree it is a draw.
    let b = board("k7/8/8/8/8/8/P7/K7 w - - 0 1");
    match tb.probe_root_board(&b).unwrap() {
        RootProbe::Move(m) => assert_eq!(m.wdl, Wdl::Draw),
        other => panic!("expected a move, got {other:?}"),
    }
    // Pawn on the 7th with the enemy king cut off: best move is promotion.
    // Kb6, Pa7 vs Kd7 — White queens (Lichess: win).
    let b = board("8/P2k4/1K6/8/8/8/8/8 w - - 0 1");
    match tb.probe_root_board(&b).unwrap() {
        RootProbe::Move(m) => {
            assert_eq!(m.wdl, Wdl::Win);
            assert_eq!(m.promotion, Some(Piece::Queen));
        }
        other => panic!("expected a move, got {other:?}"),
    }
}
