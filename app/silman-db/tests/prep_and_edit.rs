//! Verification-bar tests: prep view on a fixture opponent with a
//! hand-computable answer, and the annotation-edit round trip.

use std::io::Cursor;

use silman_db::export::export_pgn;
use silman_db::import::{import_pgn, SourceInfo, SourceKind};
use silman_db::movebin::Token;
use silman_db::prep::{prep_view, PrepOptions};
use silman_profile::Color;

fn source(name: &str, kind: SourceKind) -> SourceInfo {
    SourceInfo {
        name: name.into(),
        origin: "test".into(),
        license: "test".into(),
        kind,
    }
}

/// Villain plays the Scandinavian as Black in all four games and scores
/// terribly in it (0.5/4). By hand: after 1. e4 d5 the position (hash H)
/// is reached 4 times with score 12.5% => weakness must rank it first.
/// One master game (both 2400+) reaches the same position and must be
/// offered as prep material; a low-rated game must NOT.
#[test]
fn prep_view_hand_computable_fixture() {
    const CORPUS: &str = r#"[White "Hero"]
[Black "Villain"]
[Date "2024.01.01"]
[Result "1-0"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 c6 5. Nf3 Nf6 1-0

[White "Hero"]
[Black "Villain"]
[Date "2024.01.08"]
[Result "1-0"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 5. Nf3 c6 1-0

[White "SomeoneElse"]
[Black "Villain"]
[Date "2024.02.01"]
[Result "1-0"]

1. e4 d5 2. exd5 Nf6 3. d4 Nxd5 4. Nf3 g6 5. Be2 Bg7 1-0

[White "Fourth"]
[Black "Villain"]
[Date "2024.03.01"]
[Result "1/2-1/2"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qd6 4. d4 Nf6 5. Nf3 a6 1/2-1/2

[White "Villain"]
[Black "Other"]
[Date "2024.04.01"]
[Result "1-0"]

1. d4 d5 2. c4 e6 3. Nc3 Nf6 4. Bg5 Be7 5. e3 O-O 1-0

[Event "Masters"]
[White "GM Alpha"]
[Black "GM Beta"]
[Date "2023.05.01"]
[Result "1-0"]
[WhiteElo "2650"]
[BlackElo "2600"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 5. Nf3 Bf5 6. Bc4 e6 1-0

[Event "Club"]
[White "Patzer One"]
[Black "Patzer Two"]
[Date "2023.06.01"]
[Result "0-1"]
[WhiteElo "1500"]
[BlackElo "1450"]

1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5 4. d4 Nf6 0-1
"#;
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let st = import_pgn(
        &conn,
        &source("fixture", SourceKind::Personal),
        Cursor::new(CORPUS),
    )
    .unwrap();
    assert_eq!(st.games_imported, 7, "failures: {:?}", st.failures);

    let lines = prep_view(
        &conn,
        "Villain",
        Color::Black,
        &PrepOptions {
            max_lines: 5,
            max_master_games: 5,
            min_games: 3,
            master_min_elo: 2200,
        },
    )
    .unwrap();
    assert!(!lines.is_empty(), "must find weak lines");

    // Hand computation: as Black, Villain reached the position after 1.e4
    // (about to reply) in 4 games scoring 0.5/4 = 12.5%, and the position
    // after 1.e4 d5 2.exd5 likewise. The top line must be one of the
    // 4-game Scandinavian spots with score 12.5% and ply <= 2.
    let top = &lines[0];
    assert_eq!(top.games, 4);
    assert!((top.score_pct - 12.5).abs() < 0.01, "{}", top.score_pct);
    assert!(top.ply <= 2);
    assert!(top.opponent_moves.contains(&"d5".to_string()));

    // Master games: the GM game must appear in SOME offered line (it
    // shares the Qa5-Scandinavian path); the 1500-rated game never.
    let all_masters: Vec<String> = lines
        .iter()
        .flat_map(|l| l.master_games.iter().map(|m| m.white.clone()))
        .collect();
    assert!(
        all_masters.contains(&"GM Alpha".to_string()),
        "GM game offered: {all_masters:?}"
    );
    assert!(!all_masters.contains(&"Patzer One".to_string()));

    // The 1.d4 game Villain played as WHITE must not pollute Black prep:
    // every reported line must be one Villain reached in a Black game.
    for l in &lines {
        assert!(l.games >= 3, "no single-game noise: {l:?}");
    }
}

/// UI-entered annotation → db → exported PGN (verification bar).
#[test]
fn annotation_edit_round_trip() {
    const GAME: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. e4 e5 2. Nf3 Nc6 *\n";
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    import_pgn(&conn, &source("g", SourceKind::Personal), Cursor::new(GAME)).unwrap();

    // Simulate the UI: load tokens, attach a comment + NAG to 1.e4, and
    // add the variation (1... c5) after 1...e5.
    let (start, mut tokens) = silman_db::edit::game_tokens(&conn, 1).unwrap();
    assert_eq!(tokens.len(), 4);
    // tokens: [e4, e5, Nf3, Nc6]
    let mut board = start.clone();
    let e4 = match tokens[0] {
        Token::Move(m) => m,
        _ => panic!(),
    };
    board.play(e4);
    let c5 = silman_db::san::parse_san(&board, "c5").unwrap();
    tokens.insert(1, Token::Nag(1));
    tokens.insert(2, Token::Comment("best by test".into()));
    // After e5 (now index 3), insert the variation.
    tokens.insert(4, Token::VarStart);
    tokens.insert(5, Token::Move(c5));
    tokens.insert(6, Token::VarEnd);

    silman_db::edit::update_game_tokens(&conn, 1, &tokens).unwrap();

    let pgn = export_pgn(&conn, 1).unwrap();
    assert!(pgn.contains("1. e4 $1 {best by test}"), "{pgn}");
    assert!(pgn.contains("(1... c5)"), "{pgn}");
    assert!(pgn.contains("2. Nf3 Nc6"), "{pgn}");

    // Mainline unchanged => position index still finds the final position.
    let (hits, _) = silman_db::query::find_fen(
        &conn,
        "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    )
    .unwrap();
    assert_eq!(hits.len(), 1);

    // Second edit: extend the MAINLINE (add 3. Bb5) and verify the index
    // grows with it.
    let (start2, mut tokens2) = silman_db::edit::game_tokens(&conn, 1).unwrap();
    let mut b2 = start2.clone();
    for t in &tokens2 {
        if let Token::Move(m) = t {
            // replay mainline only (this stream's variation is bracketed)
            if !matches!(t, Token::Move(_)) {
                continue;
            }
            let _ = m;
        }
    }
    // Replay mainline properly via decode helper:
    let mainline = silman_db::movebin::mainline_of(&tokens2);
    for p in mainline {
        if let silman_db::movebin::Ply::Move(m) = p {
            b2.play(m);
        }
    }
    let bb5 = silman_db::san::parse_san(&b2, "Bb5").unwrap();
    tokens2.push(Token::Move(bb5));
    silman_db::edit::update_game_tokens(&conn, 1, &tokens2).unwrap();
    let (hits, _) = silman_db::query::find_fen(
        &conn,
        "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3",
    )
    .unwrap();
    assert_eq!(hits.len(), 1, "index rebuilt with the new mainline move");
}
