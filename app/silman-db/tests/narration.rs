//! Run-5 feedback item 2: delta narration over a full game.
//!
//! A whole annotated game is snapshot-tested, and consecutive generated
//! comments must not repeat each other — the similarity gate fails the
//! build if two adjacent narrations tell the same story twice.

use std::collections::HashSet;
use std::io::Cursor;

use silman_db::import::{import_pgn, SourceInfo, SourceKind};

/// Morphy vs Duke Karl / Count Isouard, Paris Opera 1858 — short, full of
/// persisting pressure (the classic repetition trap for a naive narrator),
/// with blunder-class NAGs on the two famous mistakes.
const OPERA_GAME: &str = "[White \"Morphy, Paul\"]\n[Black \"Duke Karl / Count Isouard\"]\n\
    [Result \"1-0\"]\n\n\
    1. e4 e5 2. Nf3 d6 3. d4 Bg4 $2 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7 \
    8. Nc3 c6 9. Bg5 b5 $4 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7 \
    14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8# 1-0\n";

fn setup() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = silman_db::db::open(&dir.path().join("t.sqlite")).unwrap();
    let src = SourceInfo {
        name: "t".into(),
        origin: "test".into(),
        license: "test".into(),
        kind: SourceKind::Personal,
    };
    let st = import_pgn(&conn, &src, Cursor::new(OPERA_GAME)).unwrap();
    assert_eq!(st.games_imported, 1, "failures: {:?}", st.failures);
    (dir, conn)
}

/// Plain Jaccard: |A∩B| / |A∪B|.
fn jaccard(a: &str, b: &str) -> f64 {
    let words = |s: &str| -> HashSet<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_lowercase)
            .collect()
    };
    let (wa, wb) = (words(a), words(b));
    let union = wa.union(&wb).count();
    if union == 0 {
        return 0.0;
    }
    wa.intersection(&wb).count() as f64 / union as f64
}

#[test]
fn full_game_narration_is_delta_driven_and_snapshot_stable() {
    let (_d, conn) = setup();
    let report = silman_db::annotate::annotate_game(&conn, 1, 100_000, 64).unwrap();
    assert!(report.comments_added > 1, "{report:?}");

    let narrations = silman_db::narrate::narrations(&conn, 1).unwrap();
    let mut plies: Vec<u32> = narrations.keys().copied().collect();
    plies.sort();

    // The repetition gate: consecutive narrations must be substantially
    // different — the delta narrator never restates a standing story.
    for pair in plies.windows(2) {
        let (a, b) = (&narrations[&pair[0]], &narrations[&pair[1]]);
        let sim = jaccard(a, b);
        assert!(
            sim < 0.6,
            "consecutive narrations at plies {} and {} are {:.0}% similar:\n--- {}\n--- {}",
            pair[0],
            pair[1],
            sim * 100.0,
            a,
            b
        );
    }

    // Blunder-class plies (3... Bg4? = ply 6, 9... b5?? = ply 18) must not
    // carry positional boilerplate: no imbalance/plan prose alongside the
    // tactical lead.
    for blunder_ply in [6u32, 18] {
        if let Some(text) = narrations.get(&blunder_ply) {
            assert!(
                !text.contains("imbalance") && !text.contains("Everything points to"),
                "blunder ply {blunder_ply} carries positional boilerplate: {text}"
            );
        }
    }

    // Snapshot the whole annotated export: any future drift in what gets
    // narrated (or repeated) shows up as a reviewable diff.
    let pgn = silman_db::export::export_pgn(&conn, 1).unwrap();
    insta::assert_snapshot!("opera_game_annotated", pgn);

    // Determinism/idempotence: re-annotating regenerates identical rows.
    silman_db::annotate::annotate_game(&conn, 1, 100_000, 64).unwrap();
    let again = silman_db::export::export_pgn(&conn, 1).unwrap();
    assert_eq!(pgn, again, "narration must be idempotent");
}

/// Run-5 item 3: the narration voice is a stored setting (meta table),
/// defaults to Coach, and re-annotating after a change regenerates the
/// same plies in the other voice.
#[test]
fn narration_voice_setting_switches_the_prose() {
    use silman_verbalize::Voice;
    let (_d, conn) = setup();

    // Coach is the default when nothing is stored.
    assert_eq!(
        silman_db::narrate::narration_voice(&conn).unwrap(),
        Voice::Coach
    );
    silman_db::annotate::annotate_game(&conn, 1, 100_000, 64).unwrap();
    let coach = silman_db::narrate::narrations(&conn, 1).unwrap();

    silman_db::narrate::set_narration_voice(&conn, Voice::Neutral).unwrap();
    assert_eq!(
        silman_db::narrate::narration_voice(&conn).unwrap(),
        Voice::Neutral
    );
    silman_db::annotate::annotate_game(&conn, 1, 100_000, 64).unwrap();
    let neutral = silman_db::narrate::narrations(&conn, 1).unwrap();

    // Same story (same narrated plies), different phrasing somewhere.
    let mut coach_plies: Vec<u32> = coach.keys().copied().collect();
    let mut neutral_plies: Vec<u32> = neutral.keys().copied().collect();
    coach_plies.sort();
    neutral_plies.sort();
    assert_eq!(
        coach_plies, neutral_plies,
        "voice must not change WHAT is narrated"
    );
    assert_ne!(coach, neutral, "voices must phrase differently");

    // Setting round-trips back to Coach as well.
    silman_db::narrate::set_narration_voice(&conn, Voice::Coach).unwrap();
    assert_eq!(
        silman_db::narrate::narration_voice(&conn).unwrap(),
        Voice::Coach
    );
    silman_db::annotate::annotate_game(&conn, 1, 100_000, 64).unwrap();
    assert_eq!(
        silman_db::narrate::narrations(&conn, 1).unwrap(),
        coach,
        "coach narration must be reproducible"
    );
}

#[test]
fn similarity_metric_sanity() {
    assert!(jaccard("the knight on d5 is strong", "the knight on d5 is strong") > 0.99);
    assert!(jaccard("White wins a pawn on e5", "Black's king is exposed") < 0.2);
}
