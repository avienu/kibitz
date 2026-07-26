//! Verification-bar tests: confirm-verdict fold-back, confirmed and
//! refuted paths (run-4 goal 3), plus legacy-analysis import provenance.

use std::io::Cursor;

use silman_db::annotate::fold_back;
use silman_db::import::{import_pgn, SourceInfo, SourceKind};

fn setup(pgn: &str) -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
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

const TRAP: &str = "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n\
    1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 4. Nxe5 Qg5 *\n";

/// Craft a done wsui-confirm job with a given verdict for the last ply.
fn plant_verdict(conn: &rusqlite::Connection, status: &str, delta: i64) {
    silman_db::annotate::annotate_game(conn, 1, 100_000, 12).unwrap();
    let (payload,): (String,) = conn
        .query_row(
            "SELECT payload FROM jobs WHERE purpose='wsui-confirm' ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    let result = serde_json::json!({
        "status": status,
        "score_cp": delta,
        "score_delta_cp": delta,
        "pv": ["c4f7", "e8d8", "e5f3"],
        "nodes": 100000,
        "engine": "Stockfish 18",
    });
    conn.execute(
        "UPDATE jobs SET status='done', result=?1 WHERE purpose='wsui-confirm'",
        [result.to_string()],
    )
    .unwrap();
    let _ = payload;
}

#[test]
fn confirmed_verdict_leads_the_comment() {
    let (_d, conn) = setup(TRAP);
    plant_verdict(&conn, "confirmed", 250);
    let report = fold_back(&conn).unwrap();
    assert!(report.folded > 0);
    assert!(report.confirmed > 0);
    // Normalize the export's 80-column soft wrapping before matching.
    let pgn = silman_db::export::export_pgn(&conn, 1)
        .unwrap()
        .replace('\n', " ");
    assert!(
        pgn.to_lowercase().contains("confirm"),
        "confirmed verdict rendered with PV:\n{pgn}"
    );
    assert!(pgn.contains("Bxf7+"), "PV rendered as SAN:\n{pgn}");

    // Idempotent: second fold does nothing.
    let again = fold_back(&conn).unwrap();
    assert_eq!(again.folded, 0);
}

#[test]
fn refuted_verdict_drops_the_alert_from_prose() {
    let (_d, conn) = setup(TRAP);
    plant_verdict(&conn, "refuted", 10);
    let before = silman_db::export::export_pgn(&conn, 1).unwrap();
    assert!(
        before.contains("loose") || before.contains("undefended") || before.contains("knight"),
        "pre-fold comment mentions the alert:\n{before}"
    );
    let report = fold_back(&conn).unwrap();
    assert!(report.refuted > 0);
    let after = silman_db::export::export_pgn(&conn, 1).unwrap();
    // The final position's comment no longer leads with the (refuted)
    // loose-knight tactic; either replaced by quieter prose or removed.
    let last_comment_mentions_e5_knight = after
        .rsplit('{')
        .next()
        .map(|c| c.contains("knight on e5"))
        .unwrap_or(false);
    assert!(
        !last_comment_mentions_e5_knight,
        "refuted alert must not lead the prose:\n{after}"
    );
}

/// Legacy engine comments become structured analyses rows at import and
/// vanish from the visible comment stream; human text is preserved.
#[test]
fn legacy_engine_comments_become_structured_rows() {
    const ANNOTATED_2011: &str =
        "[White \"sounix\"]\n[Black \"christoforo\"]\n[Result \"1-0\"]\n\n\
        1. e4 {Stockfish 2.1.1 64bit: 20:+0.30} e5 \
        {Move out of book Nf6 82% Stockfish 2.1.1 64bit: 20:+0.84} \
        2. Nf3 {Last book move} Nc6 1-0\n";
    let (_d, conn) = setup(ANNOTATED_2011);
    let rows: Vec<(i64, String, i64, i64)> = {
        let mut stmt = conn
            .prepare("SELECT ply, engine, depth, eval_cp FROM analyses WHERE kind='legacy-import' ORDER BY ply")
            .unwrap();
        let r = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        r.collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], (1, "Stockfish 2.1.1 64bit".into(), 20, 30));
    assert_eq!(rows[1], (2, "Stockfish 2.1.1 64bit".into(), 20, 84));

    let pgn = silman_db::export::export_pgn(&conn, 1).unwrap();
    assert!(!pgn.contains("2.1.1"), "engine text extracted:\n{pgn}");
    assert!(
        pgn.contains("{Move out of book Nf6 82%}"),
        "human text preserved:\n{pgn}"
    );
    assert!(pgn.contains("{Last book move}"), "{pgn}");
}
