//! Verification-bar tests: PGN round-trip semantic equality, and duplicate
//! detection across sources (TWIC-style vs personal-db headers).

use std::io::Cursor;

use silman_db::export::export_pgn;
use silman_db::import::{import_pgn, SourceInfo, SourceKind};
use silman_db::pgn::PgnReader;

fn source(name: &str) -> SourceInfo {
    SourceInfo {
        name: name.into(),
        origin: "test".into(),
        license: "test".into(),
        kind: SourceKind::Personal,
    }
}

/// Annotated, Latin-1-flavored, non-trivial PGN: nested variations, multi-
/// line comments, NAGs, promotion, en passant, castling both sides, and a
/// custom-start second game.
const ANNOTATED: &str = r#"[Event "Réti Memorial"]
[Site "Zürich SUI"]
[Date "1999.07.04"]
[Round "3"]
[White "Müller, Jörg"]
[Black "González, Iñaki"]
[Result "1-0"]
[WhiteElo "2410"]
[BlackElo "2385"]

1. e4 {King's pawn. A comment
spanning two lines.} e5 2. Nf3 $1 Nc6 (2... d6 3. d4 {Philidor} (3. Bc4))
3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 8. c3 O-O 9. h3 Nb8 $6
10. d4 Nbd7 11. c4 c6 12. cxb5 axb5 13. Nc3 Bb7 14. Bg5 b4 15. Nb1 h6
16. Bh4 c5 17. dxe5 Nxe4 18. Bxe7 Qxe7 19. exd6 Qf6 20. Nbd2 Nxd6
21. Nc4 Nxc4 22. Bxc4 Nb6 23. Ne5 Rae8 24. Bxf7+ Rxf7 25. Nxf7 Rxe1+
26. Qxe1 Kxf7 27. Qe3 Qg5 28. Qxg5 hxg5 29. b3 Ke6 30. a3 Kd6 31. axb4 cxb4
32. Ra5 Nd5 33. f3 Bc8 34. Kf2 Bf5 35. Ra7 g6 36. Ra6+ Kc5 37. Ke1 Nf4
38. g3 Nxh3 39. Kd2 Kb5 40. Rd6 1-0

[Event "Study Corner"]
[Site "?"]
[Date "2020.01.01"]
[Round "?"]
[White "Endgame, Author"]
[Black "N.N."]
[Result "1/2-1/2"]
[SetUp "1"]
[FEN "8/2P5/8/8/8/1k6/8/1K6 w - - 0 1"]

1. c8=Q Ka4 2. Qc6+ Kb4 1/2-1/2
"#;

fn tokens_of(pgn: &str) -> Vec<Vec<silman_db::pgn::PgnToken>> {
    PgnReader::new(Cursor::new(pgn))
        .map(|g| g.unwrap().tokens)
        .collect()
}

#[test]
fn import_export_round_trip_is_semantically_equal() {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let st = import_pgn(&conn, &source("annotated"), Cursor::new(ANNOTATED)).unwrap();
    assert_eq!(st.games_imported, 2, "failures: {:?}", st.failures);

    let original = tokens_of(ANNOTATED);
    for (game_id, orig_tokens) in [(1i64, &original[0]), (2i64, &original[1])] {
        let exported = export_pgn(&conn, game_id).unwrap();
        let reparsed: Vec<_> = PgnReader::new(Cursor::new(exported.as_str()))
            .map(|g| g.unwrap())
            .collect();
        assert_eq!(reparsed.len(), 1);
        let g = &reparsed[0];
        // FULL semantic equality (encoding v2): moves, comments, NAGs
        // (including !?-style suffixes normalized to NAG tokens on both
        // sides), nested variations, in identical order.
        assert_eq!(
            &g.tokens, orig_tokens,
            "game {game_id} token stream differs"
        );
    }

    // Header fidelity, including Latin-1 names and the custom start.
    let g1 = export_pgn(&conn, 1).unwrap();
    for needle in [
        "[White \"Müller, Jörg\"]",
        "[Black \"González, Iñaki\"]",
        "[Event \"Réti Memorial\"]",
        "[WhiteElo \"2410\"]",
        "[Result \"1-0\"]",
    ] {
        assert!(g1.contains(needle), "missing {needle} in:\n{g1}");
    }
    let g2 = export_pgn(&conn, 2).unwrap();
    assert!(g2.contains("[FEN \"8/2P5/8/8/8/1k6/8/1K6 w - - 0 1\"]"));
    assert!(g2.contains("[SetUp \"1\"]"));
    assert!(g2.contains("1. c8=Q"));

    // Second-order check: importing the export back in flags a duplicate.
    let st2 = import_pgn(
        &conn,
        &source("re-export"),
        Cursor::new(export_pgn(&conn, 1).unwrap()),
    )
    .unwrap();
    assert_eq!(st2.duplicates_skipped, 1);
    assert_eq!(st2.games_imported, 0);
}

/// RATIFIED design (run 3): a null move played while in check is
/// unrepresentable as a legal position; the affected line is truncated at
/// that point — never the whole game.
#[test]
fn in_check_null_truncates_the_line_not_the_game() {
    // After 2.Qh5+ Black is in check; the variation tries a null move.
    const PGN: &str = r#"[White "A"]
[Black "B"]
[Result "*"]

1. e4 f6 2. Qh5+ g6 (2... -- 3. Qe5) 3. Qxg6+ hxg6 *
"#;
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let st = import_pgn(&conn, &source("nulls"), Cursor::new(PGN)).unwrap();
    assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);

    // The mainline survives in full; the truncated (now-empty) variation
    // is dropped rather than stored as `()`.
    let exported = export_pgn(&conn, 1).unwrap();
    assert!(
        exported.contains("3. Qxg6+ hxg6"),
        "mainline intact:\n{exported}"
    );
    assert!(
        !exported.contains('('),
        "empty variation dropped:\n{exported}"
    );
    assert!(
        !exported.contains("--"),
        "in-check null not stored:\n{exported}"
    );

    // A LEGAL null in a variation is stored and round-trips as `--`.
    const LEGAL_NULL: &str = r#"[White "C"]
[Black "D"]
[Result "*"]

1. d4 d5 (1... -- 2. e4) 2. c4 *
"#;
    let st2 = import_pgn(&conn, &source("legal-null"), Cursor::new(LEGAL_NULL)).unwrap();
    assert_eq!(st2.games_imported, 1, "failures: {:?}", st2.failures);
    let exported2 = export_pgn(&conn, 2).unwrap();
    assert!(
        exported2.contains("(1... -- 2. e4)"),
        "legal null kept:\n{exported2}"
    );
}

/// The same OTB game as it appears in a TWIC issue and in a personal SCID
/// export: different Event/Site spellings, same players/date/result/moves.
#[test]
fn twic_vs_personal_db_duplicate_is_detected() {
    const PERSONAL: &str = r#"[Event "Karpov Mem"]
[Site "Poikovsky"]
[Date "2011.10.05"]
[Round "3"]
[White "Karjakin, Sergey"]
[Black "Ponomariov, Ruslan"]
[Result "1/2-1/2"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 Nf6 4. O-O Nxe4 5. d4 Nd6 6. Bxc6 dxc6 7. dxe5
Nf5 8. Qxd8+ Kxd8 1/2-1/2
"#;
    const TWIC_STYLE: &str = r#"[Event "12th Karpov Poikovsky"]
[Site "Poikovsky RUS"]
[Date "2011.10.05"]
[Round "3.2"]
[White "Karjakin, Sergey"]
[Black "Ponomariov, Ruslan"]
[Result "1/2-1/2"]
[WhiteElo "2772"]
[BlackElo "2758"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 Nf6 4. O-O Nxe4 5. d4 Nd6 6. Bxc6 dxc6 7. dxe5
Nf5 8. Qxd8+ Kxd8 1/2-1/2
"#;
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let st1 = import_pgn(&conn, &source("personal"), Cursor::new(PERSONAL)).unwrap();
    assert_eq!((st1.games_imported, st1.duplicates_skipped), (1, 0));

    let st2 = import_pgn(&conn, &source("TWIC 884"), Cursor::new(TWIC_STYLE)).unwrap();
    assert_eq!(
        (st2.games_imported, st2.duplicates_skipped),
        (0, 1),
        "different event/site/round spellings must still dedup"
    );

    // A genuinely different game between the same players on the same day
    // (one extra move) must NOT be treated as a duplicate.
    let different = PERSONAL.replace("8. Qxd8+ Kxd8 1/2-1/2", "8. Nc3 Be7 1/2-1/2");
    let st3 = import_pgn(&conn, &source("other"), Cursor::new(different)).unwrap();
    assert_eq!((st3.games_imported, st3.duplicates_skipped), (1, 0));

    // The losing TWIC copy was recorded, not deleted (decision #3).
    let dup_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM duplicates", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dup_count, 1);
    let (dup_event, kept): (String, i64) = conn
        .query_row("SELECT event, kept_game_id FROM duplicates", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(dup_event, "12th Karpov Poikovsky");
    assert_eq!(kept, 1);

    const TWIC_PGN: &str = r#"[Event "12th Karpov Poikovsky"]
[Site "Poikovsky RUS"]
[Date "2011.10.05"]
[Round "5.1"]
[White "Karjakin, Sergey"]
[Black "Morozevich, Alexander"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d3 d6 6. O-O a6 1-0
"#;
    const PERSONAL_COPY: &str = r#"[Event "Karpov Mem"]
[Site "Poikovsky"]
[Date "2011.10.05"]
[Round "5"]
[White "Karjakin, Sergey"]
[Black "Morozevich, Alexander"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d3 d6 6. O-O a6 1-0
"#;
    // Reverse order: TWIC arrives first, the personal copy later. The
    // personal source outranks TWIC, so the kept game's headers flip to
    // the personal copy and the TWIC copy moves into `duplicates`.
    let twic_src = SourceInfo {
        name: "TWIC 884".into(),
        origin: "test".into(),
        license: "test".into(),
        kind: SourceKind::Twic,
    };
    import_pgn(&conn, &twic_src, Cursor::new(TWIC_PGN)).unwrap();
    let st5 = import_pgn(&conn, &source("personal2"), Cursor::new(PERSONAL_COPY)).unwrap();
    assert_eq!(st5.duplicates_skipped, 1);
    assert_eq!(st5.duplicates_upgraded, 1, "personal outranks twic");
    let (event, kind): (String, String) = conn
        .query_row(
            "SELECT e.name, s.kind FROM games g
             JOIN events e ON e.id = g.event_id
             JOIN sources s ON s.id = g.source_id
             WHERE g.id = (SELECT MAX(id) FROM games)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        event, "Karpov Mem",
        "kept game now carries personal headers"
    );
    assert_eq!(kind, "personal");
}
