//! Repertoire Trainer integration tests: card generation from PGN
//! (counts, colors, ep-normalized hash keying), due-queue ordering, and
//! FSRS grading through the storage layer. The engine must never spawn
//! anywhere in this flow (CLAUDE.md #6).

use std::io::Cursor;

use cozy_chess::Board;
use rusqlite::Connection;
use silman_db::import::{SourceInfo, SourceKind};
use silman_db::repertoire::{
    add_line, counts, due_cards, ensure_repertoire, grade_card, import_pgn_repertoire, now_utc,
};
use silman_profile::Color;
use silman_srs::{Grade, Scheduler};

/// Two White lines sharing the first move (Italian and Ruy), plus one
/// Black Sicilian line. Card counts below are computed by hand.
const REPERTOIRE_PGN: &str = r#"[Event "White repertoire"]
[Result "*"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 *

[Event "White repertoire"]
[Result "*"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 *
"#;

const BLACK_PGN: &str = r#"[Event "Black repertoire"]
[Result "*"]

1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 *
"#;

fn test_source() -> SourceInfo {
    SourceInfo {
        name: "unit fixture".into(),
        origin: "tests/repertoire.rs".into(),
        license: "test".into(),
        kind: SourceKind::Personal,
    }
}

fn open_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    (dir, conn)
}

#[test]
fn pgn_import_generates_cards_for_the_training_color_only() {
    let (_dir, conn) = open_db();
    let st = import_pgn_repertoire(
        &conn,
        Color::White,
        "1.e4 classical",
        &test_source(),
        Cursor::new(REPERTOIRE_PGN),
    )
    .unwrap();
    assert_eq!(st.games_read, 2, "failures: {:?}", st.failures);
    assert_eq!(st.games_failed, 0);
    // Hand count: line 1 has White moves e4, Nf3, Bc4, c3 (4 cards).
    // Line 2 shares the positions before e4 and Nf3, AND its Bb5 is
    // played from the very position that already prompts Bc4 — one card
    // per position, first line in wins — so only Ba4 is new:
    // 5 distinct cards, 3 already-covered hits.
    assert_eq!(st.line.cards_added, 5);
    assert_eq!(st.line.cards_existing, 3);

    let now = now_utc(&conn).unwrap();
    let c = counts(&conn, Color::White, &now).unwrap();
    assert_eq!((c.due, c.total), (5, 5), "new cards are due immediately");
    // No Black cards were created by a White import.
    let b = counts(&conn, Color::Black, &now).unwrap();
    assert_eq!((b.due, b.total), (0, 0));

    // Every stored card has the training color to move in its FEN.
    let mut stmt = conn
        .prepare("SELECT fen, expected_san FROM repertoire_cards")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 5);
    for (fen, san) in &rows {
        let board: Board = fen.parse().unwrap();
        assert_eq!(
            board.side_to_move(),
            cozy_chess::Color::White,
            "card {san} must prompt a White move, fen {fen}"
        );
    }
}

#[test]
fn cards_are_keyed_by_the_ep_normalized_hash() {
    let (_dir, conn) = open_db();
    let st = import_pgn_repertoire(
        &conn,
        Color::White,
        "anti-sicilian",
        &test_source(),
        Cursor::new(BLACK_PGN.replace("Black", "White").as_bytes()),
    )
    .unwrap();
    assert_eq!(st.games_read, 1);

    // The position after 1.e4 c5 carries a phantom ep square when reached
    // by play; the card key must equal the hash of the conventional
    // `-` FEN (see silman_db::hash), or transposition lookups break.
    let mut played = Board::default();
    played.play("e2e4".parse().unwrap());
    played.play("c7c5".parse().unwrap());
    assert!(played.to_string().contains(" c6 "), "phantom ep in the FEN");
    let dash: Board = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2"
        .parse()
        .unwrap();
    let key = silman_db::hash::position_hash(&dash) as i64;
    assert_eq!(
        silman_db::hash::position_hash(&played) as i64,
        key,
        "normalized hashes must collide"
    );
    let san: String = conn
        .query_row(
            "SELECT expected_san FROM repertoire_cards WHERE position_hash = ?1",
            [key],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(san, "Nf3", "card found by the normalized hash");
}

#[test]
fn black_repertoire_prompts_black_moves_and_add_line_is_idempotent() {
    let (_dir, conn) = open_db();
    let st = import_pgn_repertoire(
        &conn,
        Color::Black,
        "najdorf shell",
        &test_source(),
        Cursor::new(BLACK_PGN),
    )
    .unwrap();
    // Black moves: c5, d6, cxd4, Nf6 → 4 cards.
    assert_eq!(st.line.cards_added, 4);

    let scheduler = Scheduler::default();
    let now = now_utc(&conn).unwrap();
    let due = due_cards(&conn, &scheduler, Color::Black, &now, 50).unwrap();
    assert_eq!(due.len(), 4);
    // Order tiebreak (same due timestamp): shallower positions first.
    assert_eq!(due[0].expected_san, "c5");
    assert_eq!(due[0].line_prefix, "1. e4", "prompt is the opponent move");
    assert_eq!(due[1].expected_san, "d6");
    assert_eq!(due[1].line_prefix, "1. e4 c5 2. Nf3");
    assert!(due.iter().all(|c| c.is_new));

    // Re-adding the same line creates nothing new.
    let rep_id = ensure_repertoire(&conn, Color::Black, "najdorf shell", &test_source()).unwrap();
    let sans: Vec<String> = ["e4", "c5", "Nf3", "d6", "d4", "cxd4", "Nxd4", "Nf6"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let again = add_line(&conn, rep_id, Color::Black, &Board::default(), &sans, &now).unwrap();
    assert_eq!(again.cards_added, 0);
    assert_eq!(again.cards_existing, 4);
}

#[test]
fn due_queue_orders_by_due_and_excludes_future_cards() {
    let (_dir, conn) = open_db();
    import_pgn_repertoire(
        &conn,
        Color::Black,
        "najdorf shell",
        &test_source(),
        Cursor::new(BLACK_PGN),
    )
    .unwrap();
    // Spread the due dates: c5 tomorrow (not due), d6 due long ago,
    // cxd4/Nf6 due now (creation time).
    conn.execute(
        "UPDATE repertoire_cards SET due = datetime('now', '+1 day')
         WHERE expected_san = 'c5'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE repertoire_cards SET due = datetime('now', '-3 days')
         WHERE expected_san = 'd6'",
        [],
    )
    .unwrap();
    let scheduler = Scheduler::default();
    let now = now_utc(&conn).unwrap();
    let due = due_cards(&conn, &scheduler, Color::Black, &now, 50).unwrap();
    let sans: Vec<&str> = due.iter().map(|c| c.expected_san.as_str()).collect();
    assert_eq!(
        sans,
        vec!["d6", "cxd4", "Nf6"],
        "earliest due first, c5 out"
    );
    let c = counts(&conn, Color::Black, &now).unwrap();
    assert_eq!((c.due, c.total), (3, 4));
}

#[test]
fn grading_updates_fsrs_state_due_and_lapse_history() {
    let (_dir, conn) = open_db();
    import_pgn_repertoire(
        &conn,
        Color::Black,
        "najdorf shell",
        &test_source(),
        Cursor::new(BLACK_PGN),
    )
    .unwrap();
    let scheduler = Scheduler::default();
    let now = now_utc(&conn).unwrap();
    let card = &due_cards(&conn, &scheduler, Color::Black, &now, 1).unwrap()[0];

    // First review: Good → S0(3) = 3.7145 days ≈ 320,933 seconds out.
    let g = grade_card(&conn, &scheduler, card.card_id, Grade::Good, &now).unwrap();
    assert!((g.memory.stability - 3.7145).abs() < 1e-6);
    assert!((g.interval_days - 3.7145).abs() < 1e-6);
    assert_eq!((g.reps, g.lapses), (1, 0));
    let secs: f64 = conn
        .query_row(
            "SELECT (julianday(?1) - julianday(?2)) * 86400.0",
            [&g.due, &now],
            |r| r.get(0),
        )
        .unwrap();
    assert!((secs - 320_933.0).abs() < 2.0, "due {} secs out", secs);
    // The card is no longer due.
    assert!(due_cards(&conn, &scheduler, Color::Black, &now, 50)
        .unwrap()
        .iter()
        .all(|c| c.card_id != card.card_id));

    // Failing it later is a lapse: stability collapses, lapse recorded.
    let later: String = conn
        .query_row("SELECT datetime(?1, '+4 days')", [&now], |r| r.get(0))
        .unwrap();
    let bad = grade_card(&conn, &scheduler, card.card_id, Grade::Again, &later).unwrap();
    assert!(
        bad.memory.stability < 1.5,
        "post-lapse S {}",
        bad.memory.stability
    );
    assert_eq!((bad.reps, bad.lapses), (2, 1));

    // Lapse history: both reviews logged with their grades.
    let grades: Vec<i64> = conn
        .prepare("SELECT grade FROM repertoire_reviews WHERE card_id = ?1 ORDER BY id")
        .unwrap()
        .query_map([card.card_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(grades, vec![3, 1]);

    // Product principle (CLAUDE.md #6): nothing in the trainer ever
    // touches the engine.
    assert_eq!(silman_db::engine::spawn_count(), 0);
}

/// Round-2 item 3: the grade-row previews must equal exactly what grading
/// would then persist — for new cards AND for reviewed cards with real
/// elapsed time.
#[test]
fn grade_previews_match_what_grading_does() {
    let (_dir, conn) = open_db();
    import_pgn_repertoire(
        &conn,
        Color::Black,
        "najdorf shell",
        &test_source(),
        Cursor::new(BLACK_PGN),
    )
    .unwrap();
    let scheduler = Scheduler::default();
    let now = now_utc(&conn).unwrap();

    // New card: first-rating previews are the published FSRS-4.5 initial
    // stabilities (Again clamps to the 1-day scheduling minimum).
    let card = due_cards(&conn, &scheduler, Color::Black, &now, 1).unwrap()[0].clone();
    assert!(card.is_new);
    let p = card.previews;
    assert!((p.again - 1.0).abs() < 1e-9, "again {}", p.again);
    assert!((p.hard - 1.4003).abs() < 1e-9);
    assert!((p.good - 3.7145).abs() < 1e-9);
    assert!((p.easy - 13.8206).abs() < 1e-9);
    // preview(good) == the interval grading with Good actually sets.
    let g = grade_card(&conn, &scheduler, card.card_id, Grade::Good, &now).unwrap();
    assert!((p.good - g.interval_days).abs() < 1e-12);

    // Reviewed card, seen again 4 days later: previews must still agree
    // with grading (same memory state, same elapsed-days computation).
    conn.execute(
        "UPDATE repertoire_cards SET due = datetime('now', '-1 hour') WHERE id = ?1",
        [card.card_id],
    )
    .unwrap();
    let later: String = conn
        .query_row("SELECT datetime(?1, '+4 days')", [&now], |r| r.get(0))
        .unwrap();
    let again_due = due_cards(&conn, &scheduler, Color::Black, &later, 50).unwrap();
    let seen = again_due
        .iter()
        .find(|c| c.card_id == card.card_id)
        .expect("card is due again");
    assert!(!seen.is_new);
    let p2 = seen.previews;
    assert!(p2.good > p.good, "interval grows after a success");
    assert!(p2.again <= p2.hard && p2.hard <= p2.good && p2.good <= p2.easy);
    let g2 = grade_card(&conn, &scheduler, card.card_id, Grade::Good, &later).unwrap();
    assert!(
        (p2.good - g2.interval_days).abs() < 1e-12,
        "preview {} vs graded {}",
        p2.good,
        g2.interval_days
    );
    assert_eq!(silman_db::engine::spawn_count(), 0);
}
