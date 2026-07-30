//! "Annotate this position" IPC command: static Kibitz analysis + template
//! prose for one FEN. Purely static — kibitz_core::analyze never touches
//! the engine (CLAUDE.md #6), so this is safe to call from a button press.

use serde::Serialize;

/// `explain_position` payload: the FeatureRecord (spec JSON shape, snake_case
/// fields per docs/KIBITZ_ENGINE_SPEC.md) plus the rendered prose.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    pub record: serde_json::Value,
    pub prose: String,
    /// The game-view contract (schema v3): tag, eval readout, dual-voice
    /// headline and blocks, each block with its evidence overlay set.
    pub explanation: serde_json::Value,
}

/// Optional game history for the development tracker (run 11): the SAN
/// moves that led to the explained position, from `start_fen` (standard
/// start when absent). No history → the tracker stays silent and
/// position-only callers behave exactly as before.
pub(crate) struct History<'a> {
    pub sans: &'a [String],
    pub start_fen: Option<&'a str>,
}

/// Like [`explain_position_impl`], optionally with last-move context
/// (`prev_fen` + the SAN just played) so the prose gates can tell a
/// pending recapture from a real hang, with the game history for the
/// development prior, and with the caller's openings-book verdict
/// (`in_book`: development talk defers to theory; a single quiet book
/// line renders instead — kibitz-core never learns where the book lives).
pub(crate) fn explain_position_ctx(
    fen: &str,
    voice: kibitz_verbalize::Voice,
    last: Option<(&str, &str)>,
    history: Option<History<'_>>,
    in_book: bool,
) -> Result<Explanation, String> {
    let board: cozy_chess::Board = fen.parse().map_err(|e| format!("bad FEN {fen:?}: {e:?}"))?;
    let mut record = kibitz_core::analyze(&board);

    // Development prior (run 11): replay the supplied history and fold
    // the tracker in — but only when the replay actually reaches the
    // explained position (a stale or mismatched history is ignored) and
    // the position has left the openings book.
    if !in_book {
        if let Some(history) = &history {
            let start: Option<cozy_chess::Board> = match history.start_fen {
                Some(f) => f.parse().ok(),
                None => Some(cozy_chess::Board::default()),
            };
            if let Some(start) = start {
                let mut replay = start.clone();
                let mut moves: Vec<cozy_chess::Move> = Vec::with_capacity(history.sans.len());
                let mut ok = true;
                for san in history.sans {
                    match kibitz_db::san::parse_san(&replay, san) {
                        Ok(mv) => {
                            replay.play(mv);
                            moves.push(mv);
                        }
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && replay.same_position(&board) {
                    let report = kibitz_core::development::track(&start, &moves);
                    kibitz_core::development::augment(&mut record, &report);
                }
            }
        }
    }

    let mut capture_ply = false;
    if let Some((prev_fen, san)) = last {
        if let Ok(before) = prev_fen.parse::<cozy_chess::Board>() {
            if let Ok(mv) = kibitz_db::san::parse_san(&before, san) {
                let mut check = before.clone();
                check.play(mv);
                if check == board {
                    kibitz_core::prose_gate::suppress_exchange_noise(&mut record, &before, mv);
                    let mover = before.side_to_move();
                    capture_ply = before.colors(!mover).has(mv.to)
                        || (before.piece_on(mv.from) == Some(cozy_chess::Piece::Pawn)
                            && mv.from.file() != mv.to.file()
                            && before.piece_on(mv.to).is_none());
                }
            }
        }
    }
    kibitz_core::prose_gate::suppress_escapable_attack_noise(&mut record, &board);
    let record = record;
    let mut prose = kibitz_verbalize::verbalize_voiced(&record, voice);
    if in_book {
        // The quiet line leads the prose too, mirroring narration.
        prose = format!("{} {prose}", kibitz_verbalize::book_line(voice));
    }
    let mut explanation = kibitz_verbalize::explain_in_book(&record, in_book);
    // Run 10, same rule as narration: mid-exchange the only honest advice
    // is to finish the exchange — no candidate-move chips on a capture ply.
    if capture_ply {
        explanation.suggestions.clear();
    }
    let explanation = serde_json::to_value(explanation).map_err(|e| e.to_string())?;
    let record = serde_json::to_value(&record).map_err(|e| e.to_string())?;
    Ok(Explanation {
        record,
        prose,
        explanation,
    })
}

/// The bundled openings book as a position-hash set, loaded once (the
/// dataset is a compiled-in constant, identical for every database).
fn book_contains(conn: &rusqlite::Connection, board: &cozy_chess::Board) -> bool {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static THEORY: OnceLock<HashSet<u64>> = OnceLock::new();
    let set = THEORY.get_or_init(|| kibitz_db::fingerprint::theory_set(conn).unwrap_or_default());
    set.contains(&kibitz_db::hash::position_hash(board))
}

/// Static analysis + prose for `fen` in the requested narration voice
/// ("coach" when omitted — run-5 item 3). No engine involved. `sans` +
/// `start_fen` (optional, additive) carry the game so far so the
/// development tracker can speak; without them the response is exactly
/// the old position-only one. Book state comes from the open database's
/// bundled openings set; with no database open nothing is "in book".
#[tauri::command]
pub fn explain_position(
    fen: String,
    voice: Option<String>,
    prev_fen: Option<String>,
    last_san: Option<String>,
    sans: Option<Vec<String>>,
    start_fen: Option<String>,
    state: tauri::State<'_, crate::browse::DbState>,
) -> Result<Explanation, String> {
    let voice = voice
        .as_deref()
        .map(kibitz_verbalize::Voice::from_setting)
        .unwrap_or_default();
    let last = match (prev_fen.as_deref(), last_san.as_deref()) {
        (Some(p), Some(s)) => Some((p, s)),
        _ => None,
    };
    let in_book = match (fen.parse::<cozy_chess::Board>(), state.0.lock()) {
        (Ok(board), Ok(guard)) => guard
            .as_ref()
            .is_some_and(|conn| book_contains(conn, &board)),
        _ => false,
    };
    let history = sans.as_ref().map(|sans| History {
        sans,
        start_fen: start_fen.as_deref(),
    });
    explain_position_ctx(&fen, voice, last, history, in_book)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(
        fen: &str,
        voice: kibitz_verbalize::Voice,
        last: Option<(&str, &str)>,
    ) -> Result<Explanation, String> {
        explain_position_ctx(fen, voice, last, None, false)
    }

    #[test]
    fn explains_a_position_without_an_engine() {
        use kibitz_verbalize::Voice;
        // Position after 1.e4 e5 2.Nf3 — legal, quiet.
        const FEN: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
        let e = ctx(FEN, Voice::default(), None).unwrap();
        assert!(!e.prose.is_empty());
        assert_eq!(
            e.record["schema_version"],
            kibitz_core::record::SCHEMA_VERSION
        );
        assert_eq!(e.record["side_to_move"], "black");
        assert!(e.record["engine"].is_null(), "engine stays untouched");

        // The default voice is Coach; Neutral is selectable and both
        // voices describe the same record.
        let coach = ctx(FEN, Voice::Coach, None).unwrap();
        let neutral = ctx(FEN, Voice::Neutral, None).unwrap();
        assert_eq!(e.prose, coach.prose);
        assert_eq!(coach.record, neutral.record);

        assert!(ctx("not a fen", Voice::default(), None).is_err());

        // The explanation contract rides along: dual-voice headline and
        // per-block evidence, independent of the requested prose voice.
        assert_eq!(
            coach.explanation["schemaVersion"],
            serde_json::Value::Null,
            "contract serializes snake_case like the record"
        );
        assert_eq!(
            coach.explanation["schema_version"],
            kibitz_core::record::SCHEMA_VERSION
        );
        assert!(coach.explanation["headline"]["coach"].is_string());
        assert!(coach.explanation["headline"]["neutral"].is_string());
        assert_eq!(coach.explanation, neutral.explanation);
    }

    /// Run 10: a capture ply strips the suggestion chips — mid-exchange
    /// the only honest advice is to finish the exchange.
    #[test]
    fn capture_ply_strips_suggestions() {
        use kibitz_verbalize::Voice;
        // Sveshnikov bind, White to move: quiet position, suggestions on.
        const QUIET: &str = "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7";
        let e = ctx(QUIET, Voice::default(), None).unwrap();
        assert!(
            e.explanation["suggestions"].is_array(),
            "quiet position carries suggestions: {}",
            e.explanation
        );

        // Opera game through 13.Rxd7 (a capture, recapture due): the same
        // machinery must yield NO suggestions.
        let mut board = cozy_chess::Board::default();
        let mut before = board.clone();
        for uci in [
            "e2e4", "e7e5", "g1f3", "d7d6", "d2d4", "c8g4", "d4e5", "g4f3", "d1f3", "d6e5", "f1c4",
            "g8f6", "f3b3", "d8e7", "b1c3", "c7c6", "c1g5", "b7b5", "c3b5", "c6b5", "c4b5", "b8d7",
            "e1a1", "a8d8", "d1d7",
        ] {
            before = board.clone();
            board.play(uci.parse().unwrap());
        }
        let before_fen = format!("{before}");
        let after_fen = format!("{board}");
        // Without last-move context the position DOES carry suggestions...
        let bare = ctx(&after_fen, Voice::default(), None).unwrap();
        assert!(
            bare.explanation["suggestions"].is_array(),
            "sanity: {}",
            bare.explanation
        );
        // ...and the capture context strips them.
        let e = ctx(&after_fen, Voice::default(), Some((&before_fen, "Rxd7"))).unwrap();
        assert!(
            e.explanation["suggestions"].is_null(),
            "capture ply must strip suggestions: {}",
            e.explanation["suggestions"]
        );
    }

    /// Run 11: with the game history supplied the development tracker
    /// speaks (and suggestions serve the dreams); without it the same
    /// FEN stays position-only. With `in_book` the development voice
    /// defers to theory and the quiet book line renders instead.
    #[test]
    fn history_wakes_the_development_coach_and_book_quiets_it() {
        use kibitz_verbalize::Voice;
        // After 1.e4 e5 2.Nf3, Black to move.
        const FEN: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
        let sans: Vec<String> = ["e4", "e5", "Nf3"].map(String::from).into();
        let history = || History {
            sans: &sans,
            start_fen: None,
        };

        let with =
            explain_position_ctx(FEN, Voice::default(), None, Some(history()), false).unwrap();
        let serving = |e: &Explanation| {
            e.explanation["suggestions"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|s| s["serving"].as_array().into_iter().flatten())
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert!(
            serving(&with)
                .iter()
                .any(|t| t == "CompleteDevelopment" || t == "CastleIntoSafety"),
            "development suggestions expected: {:?}",
            serving(&with)
        );
        assert!(
            with.prose.contains("develop") || with.prose.to_lowercase().contains("at home"),
            "{}",
            with.prose
        );

        // No history: the tracker is silent (position-only callers
        // unaffected).
        let without = ctx(FEN, Voice::default(), None).unwrap();
        assert!(
            !serving(&without).iter().any(|t| t == "CompleteDevelopment"),
            "{:?}",
            serving(&without)
        );

        // A mismatched history is ignored, not trusted.
        let wrong: Vec<String> = ["d4", "d5"].map(String::from).into();
        let mismatched = explain_position_ctx(
            FEN,
            Voice::default(),
            None,
            Some(History {
                sans: &wrong,
                start_fen: None,
            }),
            false,
        )
        .unwrap();
        assert!(!serving(&mismatched)
            .iter()
            .any(|t| t == "CompleteDevelopment"));

        // In book: the development voice defers to theory, and the quiet
        // book line leads the prose and appears in the explanation.
        let in_book =
            explain_position_ctx(FEN, Voice::default(), None, Some(history()), true).unwrap();
        assert!(!serving(&in_book).iter().any(|t| t == "CompleteDevelopment"));
        assert!(
            in_book.prose.starts_with("Still in the book"),
            "{}",
            in_book.prose
        );
        let blocks = in_book.explanation["blocks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let book_in_blocks = blocks.iter().any(|b| {
            b["text"]["coach"]
                .as_str()
                .is_some_and(|t| t.contains("book"))
        });
        let book_in_headline = in_book.explanation["headline"]["coach"]
            .as_str()
            .is_some_and(|t| t.contains("book"));
        assert!(
            book_in_blocks || book_in_headline,
            "{}",
            in_book.explanation
        );
    }
}
