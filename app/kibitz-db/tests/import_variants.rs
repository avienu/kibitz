//! Variant games in an import stream (2026-07-28 field report): one
//! Chess960 game in chess.com's 2012/06 export panicked the position
//! hasher (FEN round-trip of file-letter castling rights), killing the
//! network worker thread silently and wedging every subsequent sync.
//! Contract: variant games SKIP with a named failure; standard games in
//! the same stream import; nothing panics.

use std::io::BufReader;

const MIXED: &str = r#"[Event "Live Chess"]
[Site "Chess.com"]
[Date "2012.06.25"]
[White "a"]
[Black "b"]
[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0

[Event "Let's Play! - Chess960"]
[Site "Chess.com"]
[Date "2012.06.25"]
[White "Anouska11"]
[Black "sounix"]
[Result "0-1"]
[Variant "Chess960"]
[SetUp "1"]
[FEN "brkbrnnq/pppppppp/8/8/8/8/PPPPPPPP/BRKBRNNQ w EBeb - 0 1"]

1. c4 c5 2. Ne3 d6 0-1

[Event "Live Chess"]
[Site "Chess.com"]
[Date "2012.06.26"]
[White "c"]
[Black "d"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1
"#;

#[test]
fn variant_games_skip_with_named_reason_and_standard_games_import() {
    let dir = tempfile::tempdir().unwrap();
    let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let source = kibitz_db::import::SourceInfo {
        name: "mixed month".into(),
        origin: "test".into(),
        license: "test".into(),
        kind: kibitz_db::import::SourceKind::Online,
    };
    let stats =
        kibitz_db::import::import_pgn(&conn, &source, BufReader::new(MIXED.as_bytes())).unwrap();
    assert_eq!(stats.games_imported, 2, "both standard games import");
    assert_eq!(stats.games_failed, 1, "the 960 game skips, named");
    assert!(
        stats.failures.iter().any(|f| f.contains("Chess960")),
        "failure names the variant: {:?}",
        stats.failures
    );
}

/// The hasher itself must never panic on a 960 board, however it is
/// reached (position search on a pasted FEN takes arbitrary input).
#[test]
fn position_hash_survives_a_chess960_board() {
    let board: cozy_chess::Board = "brkbrnnq/pppppppp/8/8/8/8/PPPPPPPP/BRKBRNNQ w EBeb - 0 1"
        .parse()
        .unwrap();
    let _ = kibitz_db::hash::position_hash(&board); // must not panic
}
