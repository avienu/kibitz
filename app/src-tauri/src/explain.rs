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

pub(crate) fn explain_position_impl(
    fen: &str,
    voice: kibitz_verbalize::Voice,
) -> Result<Explanation, String> {
    explain_position_ctx(fen, voice, None)
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
    if let Some((prev_fen, san)) = last {
        if let Ok(before) = prev_fen.parse::<cozy_chess::Board>() {
            if let Ok(mv) = kibitz_db::san::parse_san(&before, san) {
                let mut check = before.clone();
                check.play(mv);
                if check == board {
                    kibitz_core::prose_gate::suppress_exchange_noise(&mut record, &before, mv);
                }
            }
        }
    }
    kibitz_core::prose_gate::suppress_escapable_attack_noise(&mut record, &board);
    let record = record;
    let prose = kibitz_verbalize::verbalize_voiced(&record, voice);
    let explanation =
        serde_json::to_value(kibitz_verbalize::explain(&record)).map_err(|e| e.to_string())?;
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
        let e = explain_position_impl(FEN, Voice::default()).unwrap();
        assert!(!e.prose.is_empty());
        assert_eq!(
            e.record["schema_version"],
            kibitz_core::record::SCHEMA_VERSION
        );
        assert_eq!(e.record["side_to_move"], "black");
        assert!(e.record["engine"].is_null(), "engine stays untouched");

        // The default voice is Coach; Neutral is selectable and both
        // voices describe the same record.
        let coach = explain_position_impl(FEN, Voice::Coach).unwrap();
        let neutral = explain_position_impl(FEN, Voice::Neutral).unwrap();
        assert_eq!(e.prose, coach.prose);
        assert_eq!(coach.record, neutral.record);

        assert!(explain_position_impl("not a fen", Voice::default()).is_err());

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
}
