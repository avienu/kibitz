//! "Annotate this position" IPC command: static Silman analysis + template
//! prose for one FEN. Purely static — silman_core::analyze never touches
//! the engine (CLAUDE.md #6), so this is safe to call from a button press.

use serde::Serialize;

/// `explain_position` payload: the FeatureRecord (spec JSON shape, snake_case
/// fields per docs/SILMAN_ENGINE_SPEC.md) plus the rendered prose.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    pub record: serde_json::Value,
    pub prose: String,
}

pub(crate) fn explain_position_impl(
    fen: &str,
    voice: silman_verbalize::Voice,
) -> Result<Explanation, String> {
    let board: cozy_chess::Board = fen.parse().map_err(|e| format!("bad FEN {fen:?}: {e:?}"))?;
    let record = silman_core::analyze(&board);
    let prose = silman_verbalize::verbalize_voiced(&record, voice);
    let record = serde_json::to_value(&record).map_err(|e| e.to_string())?;
    Ok(Explanation { record, prose })
}

/// Static analysis + prose for `fen` in the requested narration voice
/// ("coach" when omitted — run-5 item 3). No engine involved.
#[tauri::command]
pub fn explain_position(fen: String, voice: Option<String>) -> Result<Explanation, String> {
    let voice = voice
        .as_deref()
        .map(silman_verbalize::Voice::from_setting)
        .unwrap_or_default();
    explain_position_impl(&fen, voice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_a_position_without_an_engine() {
        use silman_verbalize::Voice;
        // Position after 1.e4 e5 2.Nf3 — legal, quiet.
        const FEN: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
        let e = explain_position_impl(FEN, Voice::default()).unwrap();
        assert!(!e.prose.is_empty());
        assert_eq!(
            e.record["schema_version"],
            silman_core::record::SCHEMA_VERSION
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
    }
}
