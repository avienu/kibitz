//! silman-db: GPL-3.0 app-layer database core.
//!
//! SQLite schema/migrations, streaming PGN importer with duplicate
//! detection, versioned binary move encoding, Zobrist position index and
//! position search. Lives in `app/` (GPL layer) per ARCHITECTURE.md; the
//! BSD crates must never depend on this.

pub mod annotate;
pub mod db;
pub mod eco;
pub mod edit;
pub mod engine;
pub mod export;
pub mod fingerprint;
pub mod hash;
pub mod import;
pub mod import_si4;
pub mod jobs;
pub mod legacy;
pub mod movebin;
pub mod net;
pub mod pgn;
pub mod prep;
pub mod profile;
pub mod query;
pub mod san;
pub mod twic;

#[cfg(test)]
mod tests {
    use crate::import::{import_pgn, SourceInfo, SourceKind};
    use crate::query::{find_fen, stats};
    use std::io::Cursor;

    const FIXTURE: &str = r#"[Event "Casual Game"]
[Site "Paris FRA"]
[Date "1858.11.02"]
[White "Morphy, Paul"]
[Black "Duke Karl / Count Isouard"]
[Result "1-0"]

1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7
8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7
14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0

[Event "Test Miniature"]
[White "Someone"]
[Black "Someone Else"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1

[Event "Broken Game"]
[White "Bad"]
[Black "Data"]
[Result "*"]

1. e4 e5 2. Nf3 Qxg8 3. d4 *

[Event "Casual Game re-export"]
[Site "Paris"]
[Date "1858.11.02"]
[White "Morphy, Paul"]
[Black "Duke Karl / Count Isouard"]
[Result "1-0"]

1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7
8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7
14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0
"#;

    #[test]
    fn end_to_end_import_and_position_search() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("test.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        let st = import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        assert_eq!(st.games_imported, 2, "failures: {:?}", st.failures);
        assert_eq!(st.duplicates_skipped, 1, "re-exported Morphy game is a dup");
        assert_eq!(st.games_failed, 1, "illegal Qxg8 game rejected");
        assert!(st.positions_indexed > 30);

        let db_stats = stats(&conn).unwrap();
        assert_eq!(db_stats.games, 2);
        assert_eq!(db_stats.sources, 1);

        // Position right after 1.e4 (a double push: the played position
        // carries a phantom ep file that the normalized hash must ignore).
        let (hits, _) = find_fen(
            &conn,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ply, 1);

        // Position after 1.e4 e5 2.Nf3 — reached by the Morphy game only.
        let (hits, _) = find_fen(
            &conn,
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].white, "Morphy, Paul");
        assert_eq!(hits[0].ply, 3);

        // Final position of the fool's-mate miniature.
        let (hits, _) = find_fen(
            &conn,
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].result, "0-1");

        // A position no game reached.
        let (hits, _) = find_fen(
            &conn,
            "rnbqkbnr/pppppppp/8/8/7P/8/PPPPPPP1/RNBQKBNR b KQkq - 0 1",
        )
        .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn opening_tree_counts_validated_by_hand() {
        // Fixture: Morphy game (1.e4, White wins), miniature (1.f3, Black
        // wins), broken game (rejected), duplicate (skipped). From the
        // start position the tree must therefore be exactly:
        //   e4: 1 game, 1-0  |  f3: 1 game, 0-1
        // and after 1.e4 e5 2.Nf3 (Morphy only): d6: 1 game, 1-0.
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("test.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();

        let (tree, _) = crate::query::opening_tree(
            &conn,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(tree.len(), 2);
        let e4 = tree.iter().find(|m| m.san == "e4").unwrap();
        assert_eq!(
            (e4.count, e4.white_wins, e4.draws, e4.black_wins),
            (1, 1, 0, 0)
        );
        let f3 = tree.iter().find(|m| m.san == "f3").unwrap();
        assert_eq!(
            (f3.count, f3.white_wins, f3.draws, f3.black_wins),
            (1, 0, 0, 1)
        );

        let (tree, _) = crate::query::opening_tree(
            &conn,
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
        )
        .unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].san, "d6");

        // ECO tagging via the bundled dataset: the Morphy game is a Philidor.
        let eco: String = conn
            .query_row("SELECT eco FROM games WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(eco.starts_with("C41"), "expected Philidor C41, got {eco}");
    }

    #[test]
    fn movetext_blob_decodes_back_to_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("test.sqlite")).unwrap();
        let source = SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        };
        import_pgn(&conn, &source, Cursor::new(FIXTURE)).unwrap();
        let (blob, ply_count): (Vec<u8>, i64) = conn
            .query_row(
                "SELECT movetext, ply_count FROM games WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // v2: one byte per ply plus the END token for an unannotated game.
        assert_eq!(blob.len() as i64, ply_count + 1);
        let moves = crate::movebin::decode_game(&cozy_chess::Board::default(), &blob).unwrap();
        assert_eq!(moves.len(), 33, "Opera game is 33 plies");
    }
}
