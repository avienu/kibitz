//! Offline tests for the LLM verbalizer (feature `llm`): deterministic
//! prompt snapshot, grounded output accepted verbatim, hallucinated squares
//! and moves triggering the total template fallback, and transport errors
//! doing the same. No network is touched anywhere.
//!
//! Run with: `cargo test -p kibitz-verbalize --features llm`

#![cfg(feature = "llm")]

use std::collections::BTreeMap;

use kibitz_core::record::{
    AlertKind, EngineCheck, EngineCheckStatus, Favors, FeatureRecord, Imbalance, ImbalanceKind,
    Magnitude, Phase, PlanHint, Provenance, Severity, SideColor, TacticAlert, WsuiReport,
    SCHEMA_VERSION,
};
use kibitz_verbalize::llm::{
    build_prompt, build_prompt_voiced, validate, FallbackReason, LlmTransport, LlmVerbalizer,
    TransportError, VerbalizeMode,
};
use kibitz_verbalize::{verbalize, verbalize_voiced, Verbalizer, Voice};

/// Transport double returning a canned result; never touches the network.
struct FakeTransport(Result<String, TransportError>);

impl LlmTransport for FakeTransport {
    fn complete(&self, _system: &str, _user: &str) -> Result<String, TransportError> {
        self.0.clone()
    }
}

/// Tactical middlegame fixture (same position family as the template-mode
/// tests): Black's c6-knight is attacked by the e5-knight and b5-bishop and
/// held only by the b7-pawn; engine-confirmed PV Bxc6 bxc6 Nxc6. White is
/// to move; castling short (O-O) is legal in the FEN but appears nowhere in
/// the record, which exercises the legality branch of the move rule.
fn fixture_record() -> FeatureRecord {
    let mut evidence = BTreeMap::new();
    evidence.insert("isolated".to_string(), serde_json::json!(["d5"]));
    evidence.insert("half_open_files".to_string(), serde_json::json!(["e"]));
    FeatureRecord {
        schema_version: SCHEMA_VERSION,
        fen: "r1bqk2r/pp2bppp/2n2n2/1B1pN3/3P4/2N5/PPP2PPP/R1BQK2R w KQkq - 0 9".into(),
        side_to_move: SideColor::White,
        phase: Phase::Middlegame,
        wsui: WsuiReport {
            alerts: vec![TacticAlert {
                kind: AlertKind::InadequatelyDefended,
                side: SideColor::Black,
                target: Some("c6".into()),
                attackers: vec!["e5".into(), "b5".into()],
                defenders: vec!["b7".into()],
                see: Some(200),
                severity: Severity::High,
                detail: Some("overloaded-defender".into()),
                engine_check: Some(EngineCheck {
                    status: EngineCheckStatus::Confirmed,
                    pv: vec!["Bxc6".into(), "bxc6".into(), "Nxc6".into()],
                    score_delta_cp: Some(190),
                    mate_in: None,
                    budget_nodes: 2_000_000,
                }),
            }],
            screen_fired: true,
        },
        imbalances: vec![Imbalance {
            kind: ImbalanceKind::PawnStructure,
            favors: Favors::White,
            magnitude: Magnitude::Clear,
            evidence,
            plans: vec![PlanHint {
                hint: "BlockadeThenPressure".into(),
                squares: vec!["d4".into(), "d5".into()],
            }],
        }],
        composite_plans: vec![],
        maneuvers: vec![],
        schemes: vec![],
        engine: None,
        provenance: Provenance {
            generator: "kibitz-core".into(),
            version: "0.1.0".into(),
        },
    }
}

#[test]
fn prompt_construction_is_deterministic_snapshot() {
    let record = fixture_record();
    let (system, user) = build_prompt(&record);
    let (system2, user2) = build_prompt(&record);
    assert_eq!(system, system2);
    assert_eq!(user, user2);
    insta::assert_snapshot!(format!("SYSTEM:\n{system}\n\nUSER:\n{user}"));
}

/// Run-5 item 3: the prompt names the requested voice's style, and the
/// template fallback is rendered in that same voice.
#[test]
fn prompt_and_fallback_respect_the_requested_voice() {
    let record = fixture_record();
    let (coach_system, coach_user) = build_prompt_voiced(&record, Voice::Coach);
    let (neutral_system, neutral_user) = build_prompt_voiced(&record, Voice::Neutral);
    assert!(coach_system.contains("coaching voice"), "{coach_system}");
    assert!(neutral_system.contains("neutral voice"), "{neutral_system}");
    assert_ne!(coach_system, neutral_system);
    assert_eq!(coach_user, neutral_user, "voice only changes the style");
    // The default prompt is the Coach prompt.
    let (default_system, _) = build_prompt(&record);
    assert_eq!(default_system, coach_system);

    // A transport failure falls back to template prose IN the same voice.
    let failing = || FakeTransport(Err(TransportError::new("down")));
    let coach_out = LlmVerbalizer::with_voice(failing(), Voice::Coach).verbalize_checked(&record);
    let neutral_out =
        LlmVerbalizer::with_voice(failing(), Voice::Neutral).verbalize_checked(&record);
    assert_eq!(coach_out.text, verbalize_voiced(&record, Voice::Coach));
    assert_eq!(neutral_out.text, verbalize_voiced(&record, Voice::Neutral));
    assert_ne!(coach_out.text, neutral_out.text);
}

#[test]
fn grounded_output_passes_validation_and_is_used_verbatim() {
    let record = fixture_record();
    let grounded = "Black's knight on c6 is in serious trouble: it is attacked \
by the pieces on e5 and b5 and held together only by the b7-pawn.\n\n\
White's pawn structure is clearly better; the black pawn on d5 is isolated.\n\n\
White should blockade on d4 and then press against d5. The line Bxc6 bxc6 Nxc6 \
wins material.";
    let out =
        LlmVerbalizer::new(FakeTransport(Ok(grounded.to_string()))).verbalize_checked(&record);
    assert_eq!(out.mode, VerbalizeMode::Llm);
    assert_eq!(out.text, grounded);
    // The Verbalizer trait returns the same prose.
    assert_eq!(
        LlmVerbalizer::new(FakeTransport(Ok(grounded.to_string()))).verbalize(&record),
        grounded
    );
}

#[test]
fn legal_move_absent_from_record_is_accepted_by_the_legality_branch() {
    // O-O and Nxd5 appear nowhere in the record but are legal for White in
    // the record's FEN, so the move rule admits them.
    let record = fixture_record();
    let text = "White can simply castle with O-O, or grab a pawn with Nxd5.";
    assert_eq!(validate(text, &record), Ok(()));
}

#[test]
fn hallucinated_square_triggers_template_fallback() {
    let record = fixture_record();
    let hallucinated = "The rook on h3 dominates the position.";
    let out =
        LlmVerbalizer::new(FakeTransport(Ok(hallucinated.to_string()))).verbalize_checked(&record);
    assert_eq!(
        out.mode,
        VerbalizeMode::TemplateFallback(FallbackReason::UngroundedSquare("h3".into()))
    );
    // The fallback is the full template rendering, never partial LLM prose.
    assert_eq!(out.text, verbalize(&record));
    assert!(!out.text.contains("h3"));
}

#[test]
fn hallucinated_move_triggers_template_fallback() {
    // Qxf7# is not in the record and is illegal in the position (the white
    // queen on d1 cannot reach f7).
    let record = fixture_record();
    let hallucinated = "White wins on the spot with Qxf7#.";
    let out =
        LlmVerbalizer::new(FakeTransport(Ok(hallucinated.to_string()))).verbalize_checked(&record);
    assert_eq!(
        out.mode,
        VerbalizeMode::TemplateFallback(FallbackReason::UngroundedMove("Qxf7#".into()))
    );
    assert_eq!(out.text, verbalize(&record));
}

#[test]
fn hallucinated_bare_square_move_is_caught_by_the_square_rule() {
    // "a4" parses as a bare square, not a SAN token; the square rule rejects
    // it because a4 appears nowhere in the record.
    let record = fixture_record();
    let out = LlmVerbalizer::new(FakeTransport(Ok("Play a4 at once.".to_string())))
        .verbalize_checked(&record);
    assert_eq!(
        out.mode,
        VerbalizeMode::TemplateFallback(FallbackReason::UngroundedSquare("a4".into()))
    );
}

#[test]
fn transport_error_triggers_template_fallback() {
    let record = fixture_record();
    let error = TransportError::new("HTTP 500 from provider");
    let out = LlmVerbalizer::new(FakeTransport(Err(error.clone()))).verbalize_checked(&record);
    assert_eq!(
        out.mode,
        VerbalizeMode::TemplateFallback(FallbackReason::Transport(error))
    );
    assert_eq!(out.text, verbalize(&record));
}

#[test]
fn empty_output_triggers_template_fallback() {
    let record = fixture_record();
    let out = LlmVerbalizer::new(FakeTransport(Ok("  \n ".to_string()))).verbalize_checked(&record);
    assert_eq!(
        out.mode,
        VerbalizeMode::TemplateFallback(FallbackReason::EmptyOutput)
    );
    assert_eq!(out.text, verbalize(&record));
}
