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

/// Like [`explain_position_impl`], optionally with last-move context
/// (`prev_fen` + the SAN just played) so the prose gates can tell a
/// pending recapture from a real hang.
pub(crate) fn explain_position_ctx(
    fen: &str,
    voice: kibitz_verbalize::Voice,
    last: Option<(&str, &str)>,
) -> Result<Explanation, String> {
    let board: cozy_chess::Board = fen.parse().map_err(|e| format!("bad FEN {fen:?}: {e:?}"))?;
    let mut record = kibitz_core::analyze(&board);
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
    let prose = kibitz_verbalize::verbalize_voiced(&record, voice);
    let mut explanation = kibitz_verbalize::explain(&record);
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

/// Static analysis + prose for `fen` in the requested narration voice
/// ("coach" when omitted — run-5 item 3). No engine involved.
#[tauri::command]
pub fn explain_position(
    fen: String,
    voice: Option<String>,
    prev_fen: Option<String>,
    last_san: Option<String>,
) -> Result<Explanation, String> {
    let voice = voice
        .as_deref()
        .map(kibitz_verbalize::Voice::from_setting)
        .unwrap_or_default();
    let last = match (prev_fen.as_deref(), last_san.as_deref()) {
        (Some(p), Some(s)) => Some((p, s)),
        _ => None,
    };
    explain_position_ctx(&fen, voice, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_a_position_without_an_engine() {
        use kibitz_verbalize::Voice;
        // Position after 1.e4 e5 2.Nf3 — legal, quiet.
        const FEN: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
        let e = explain_position_ctx(FEN, Voice::default(), None).unwrap();
        assert!(!e.prose.is_empty());
        assert_eq!(
            e.record["schema_version"],
            kibitz_core::record::SCHEMA_VERSION
        );
        assert_eq!(e.record["side_to_move"], "black");
        assert!(e.record["engine"].is_null(), "engine stays untouched");

        // The default voice is Coach; Neutral is selectable and both
        // voices describe the same record.
        let coach = explain_position_ctx(FEN, Voice::Coach, None).unwrap();
        let neutral = explain_position_ctx(FEN, Voice::Neutral, None).unwrap();
        assert_eq!(e.prose, coach.prose);
        assert_eq!(coach.record, neutral.record);

        assert!(explain_position_ctx("not a fen", Voice::default(), None).is_err());

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
        let e = explain_position_ctx(QUIET, Voice::default(), None).unwrap();
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
        let bare = explain_position_ctx(&after_fen, Voice::default(), None).unwrap();
        assert!(
            bare.explanation["suggestions"].is_array(),
            "sanity: {}",
            bare.explanation
        );
        // ...and the capture context strips them.
        let e = explain_position_ctx(&after_fen, Voice::default(), Some((&before_fen, "Rxd7")))
            .unwrap();
        assert!(
            e.explanation["suggestions"].is_null(),
            "capture ply must strip suggestions: {}",
            e.explanation["suggestions"]
        );
    }
}
