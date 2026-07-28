//! Delta narration (run-5 feedback item 2): the shared narrator behind
//! both batch annotation and verdict fold-back.
//!
//! An annotation narrates what the MOVE CHANGED — a new tactical alert,
//! a theme that appeared or grew, a plan that came into focus — never a
//! restatement of everything true about the position. Standing themes
//! are restated only when the game changes phase. Blunder-class moves
//! (?? / ?) lead tactically with positional boilerplate suppressed.
//!
//! Generated prose lives in the `narrations` table, one row per mainline
//! ply, regenerated wholesale on every call. Human comments in the
//! movetext are never touched.

use std::collections::{BTreeMap, HashMap, HashSet};

use kibitz_core::record::{EngineCheck, EngineCheckStatus, Magnitude, Severity};
use kibitz_verbalize::Voice;
use rusqlite::{Connection, OptionalExtension};

use crate::movebin::Token;

/// `meta` key holding the user's narration voice ("coach" / "neutral").
/// The `meta` key/value table is this codebase's existing minimal config
/// mechanism (position_hash_version, encoding_version live there too), so
/// the voice setting needs no schema migration.
const VOICE_META_KEY: &str = "narration_voice";

/// The stored narration voice, defaulting to [`Voice::Coach`] when the
/// setting is absent or unrecognized (run-5 item 3: Coach is the default).
pub fn narration_voice(conn: &Connection) -> anyhow::Result<Voice> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [VOICE_META_KEY],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored
        .as_deref()
        .map(Voice::from_setting)
        .unwrap_or_default())
}

/// Persist the narration voice. Callers regenerate narrations themselves
/// (the next annotate/fold-back pass picks the new voice up).
pub fn set_narration_voice(conn: &Connection, voice: Voice) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![VOICE_META_KEY, voice.as_str()],
    )?;
    Ok(())
}

/// A completed wsui-confirm verdict for one mainline ply.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub status: EngineCheckStatus,
    pub pv_uci: Vec<String>,
    pub score_delta_cp: Option<i32>,
    pub mate_in: Option<i32>,
    pub nodes: u64,
    /// Engine-cleared static candidate moves for this position (run 11):
    /// the wsui-confirm job's cursory suggestion review. `Some` even when
    /// empty (the engine ran and refuted everything); `None` when the job
    /// predates the review or had nothing to verify — then the static
    /// veto governs.
    pub cleared_suggestions: Option<Vec<String>>,
}

/// Load every completed verdict for a game, keyed by mainline ply.
pub fn load_verdicts(conn: &Connection, game_id: i64) -> anyhow::Result<HashMap<u32, Verdict>> {
    let mut out = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT payload, result FROM jobs
         WHERE purpose = 'wsui-confirm' AND status = 'done' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (payload, result) = row?;
        let p: crate::jobs::EnginePayload = serde_json::from_str(&payload)?;
        if p.game_id != Some(game_id) {
            continue;
        }
        let Some(ply) = p.ply else { continue };
        let v: serde_json::Value = serde_json::from_str(&result)?;
        let status = match v["status"].as_str() {
            Some("confirmed") => EngineCheckStatus::Confirmed,
            Some("refuted") => EngineCheckStatus::Refuted,
            _ => EngineCheckStatus::UnclearAtBudget,
        };
        let mate_in = v["mate_for_beneficiary"].as_i64().map(|m| m as i32);
        out.insert(
            ply,
            Verdict {
                status,
                pv_uci: v["pv"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|u| u.as_str().map(str::to_string))
                    .collect(),
                // A mate score must never render as material (run-5 bug 1).
                score_delta_cp: if mate_in.is_some() {
                    None
                } else {
                    v["score_delta_cp"].as_i64().map(|x| x as i32)
                },
                mate_in,
                nodes: v["nodes"].as_u64().unwrap_or(0),
                cleared_suggestions: v["cleared_suggestions"].as_array().map(|list| {
                    list.iter()
                        .filter_map(|u| u.as_str().map(str::to_string))
                        .collect()
                }),
            },
        );
    }
    Ok(out)
}

/// NAG values marking a blunder-class move (? = 2, ?? = 4).
fn is_blunder_nag(n: u8) -> bool {
    n == 2 || n == 4
}

fn alert_key(a: &kibitz_core::record::TacticAlert) -> String {
    // A king that wanders square to square is ONE story, not a new alert
    // per square (run-6 residual): key WeakKing by side, everything else
    // by its target square.
    if a.kind == kibitz_core::record::AlertKind::WeakKing {
        format!("{:?}@{:?}", a.kind, a.side)
    } else {
        format!("{:?}@{:?}", a.kind, a.target)
    }
}

fn theme_key(i: &kibitz_core::record::Imbalance) -> String {
    format!("{:?}:{:?}", i.kind, i.favors)
}

fn plan_key(p: &kibitz_core::record::CompositePlan) -> String {
    format!("{}:{:?}", p.target, p.favors)
}

/// Regenerate the full narration set for one game from scratch, rendering
/// prose in `voice` (callers read the stored setting via
/// [`narration_voice`]).
///
/// Deterministic and idempotent: the same game + verdicts + voice always
/// produce the same rows, so calling after every new verdict batch is safe.
/// Returns the number of narrated plies.
pub fn narrate_game(
    conn: &Connection,
    game_id: i64,
    verdicts: &HashMap<u32, Verdict>,
    max_comments: u32,
    voice: Voice,
) -> anyhow::Result<u32> {
    let (start, tokens) = crate::edit::game_tokens(conn, game_id)?;

    let mut board = start.clone();
    let mut depth = 0u32;
    let mut ply = 0u32;
    let mut rows: BTreeMap<u32, String> = BTreeMap::new();

    // Standing-story state carried across plies.
    let mut prev_alerts: HashSet<String> = HashSet::new();
    // An alert that vanishes for a moment (its attacker was captured and
    // instantly replaced) must not re-narrate verbatim a ply later; only
    // after this many plies does the same story count as news again.
    const REARISE_WINDOW: u32 = 8;
    let mut alert_last_narrated: HashMap<String, u32> = HashMap::new();
    let mut prev_themes: HashMap<String, Magnitude> = HashMap::new();
    let mut narrated_plans: HashSet<String> = HashSet::new();
    let mut prev_phase: Option<kibitz_core::record::Phase> = None;
    let mut last_verdict_story = String::new();

    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::VarStart => depth += 1,
            Token::VarEnd => depth = depth.saturating_sub(1),
            Token::Null if depth == 0 => break,
            Token::Move(mv) if depth == 0 => {
                let board_before = board.clone();
                board.play(*mv);
                ply += 1;
                if board.status() != cozy_chess::GameStatus::Ongoing {
                    // The game is over; narrating hanging pieces in a
                    // checkmate diagram helps nobody.
                    break;
                }

                let mut record = kibitz_core::analyze(&board);
                record
                    .wsui
                    .alerts
                    .retain(|a| a.severity >= Severity::Medium);
                // Coach-voice noise gates (run 9): a sound capture's
                // pending recapture is not a hang, the one-ply material
                // spike is not a material edge, and an attacked heavy
                // piece with a flight square is tempo, not tactics.
                kibitz_core::prose_gate::suppress_exchange_noise(&mut record, &board_before, *mv);
                kibitz_core::prose_gate::suppress_escapable_attack_noise(&mut record, &board);

                // Merge this ply's engine verdict, if any.
                let mut verdict_status = None;
                if let Some(v) = verdicts.get(&ply) {
                    verdict_status = Some(v.status);
                    match v.status {
                        EngineCheckStatus::Refuted => {
                            // The tactic does not work: drop the lead alert.
                            if !record.wsui.alerts.is_empty() {
                                record.wsui.alerts.remove(0);
                            }
                        }
                        _ => {
                            if let Some(top) = record.wsui.alerts.first_mut() {
                                top.engine_check = Some(EngineCheck {
                                    status: v.status,
                                    pv: pv_to_san(&board, &v.pv_uci),
                                    score_delta_cp: v.score_delta_cp,
                                    mate_in: v.mate_in,
                                    budget_nodes: v.nodes,
                                });
                            }
                        }
                    }
                    record.wsui.screen_fired = !record.wsui.alerts.is_empty();
                }

                // Current full story, before delta filtering.
                let cur_alerts: HashSet<String> =
                    record.wsui.alerts.iter().map(alert_key).collect();
                let cur_themes: HashMap<String, Magnitude> = record
                    .imbalances
                    .iter()
                    .filter(|i| i.magnitude >= Magnitude::Clear)
                    .map(|i| (theme_key(i), i.magnitude))
                    .collect();
                let phase_boundary = prev_phase.is_some_and(|p| p != record.phase);

                // Blunder-class move: a ? or ?? NAG directly follows it.
                let blunder = tokens[idx + 1..]
                    .iter()
                    .take_while(|t| matches!(t, Token::Nag(_)))
                    .any(|t| matches!(t, Token::Nag(n) if is_blunder_nag(*n)));

                // ---- Delta filtering ----
                // Alerts: narrate when NEW, or when an engine verdict just
                // landed on a persisting one (but the same verdict story
                // only once, not at every ply it keeps firing).
                let verdict_story = record
                    .wsui
                    .alerts
                    .first()
                    .filter(|_| verdict_status.is_some())
                    .map(|a| format!("{}:{:?}", alert_key(a), verdict_status.unwrap()))
                    .unwrap_or_default();
                let fresh_verdict =
                    !verdict_story.is_empty() && verdict_story != last_verdict_story;
                record.wsui.alerts.retain(|a| {
                    let key = alert_key(a);
                    let recently_told = alert_last_narrated
                        .get(&key)
                        .is_some_and(|&at| ply - at <= REARISE_WINDOW);
                    (!prev_alerts.contains(&key) && !recently_told)
                        || (fresh_verdict && a.engine_check.is_some())
                });
                record.wsui.screen_fired = !record.wsui.alerts.is_empty();

                // Themes: narrate when new or when the magnitude moved; at
                // a phase boundary restate everything still standing.
                record.imbalances.retain(|i| {
                    i.magnitude >= Magnitude::Clear
                        && (phase_boundary || prev_themes.get(&theme_key(i)) != Some(&i.magnitude))
                });

                // Plans: the top composite is narrated once when it forms
                // (or re-forms after a phase change).
                if phase_boundary {
                    narrated_plans.clear();
                }
                record
                    .composite_plans
                    .retain(|p| !narrated_plans.contains(&plan_key(p)));

                // A blunder-class move leads tactically: suppress the
                // positional boilerplate entirely.
                if blunder {
                    record.imbalances.clear();
                    record.composite_plans.clear();
                }

                let has_content = record.wsui.screen_fired
                    || !record.imbalances.is_empty()
                    || !record.composite_plans.is_empty();
                if has_content && rows.len() < max_comments as usize {
                    let mut sections = kibitz_verbalize::verbalize_sections_voiced(&record, voice);
                    // Candidate-move closing (run 10): one "what to play"
                    // sentence at plies where plans are narrated — but
                    // never on a capture ply, where the only honest
                    // advice is to finish the exchange in progress.
                    let capture_ply = board_before.colors(!board_before.side_to_move()).has(mv.to)
                        || (board_before.piece_on(mv.from) == Some(cozy_chess::Piece::Pawn)
                            && mv.from.file() != mv.to.file()
                            && board_before.piece_on(mv.to).is_none());
                    if !sections.plans.is_empty() && !capture_ply {
                        // Engine-verification context (run 11): at plies
                        // where the wsui-confirm job reviewed the static
                        // candidates, only cleared moves render; with no
                        // engine review the static veto drops marked ones.
                        let cleared = verdicts
                            .get(&ply)
                            .and_then(|v| v.cleared_suggestions.as_deref());
                        if let Some(closing) =
                            kibitz_verbalize::suggestion_closing_verified(&record, voice, cleared)
                        {
                            sections.plans.push(' ');
                            sections.plans.push_str(&closing);
                        }
                    }
                    let text = [sections.tactics, sections.imbalances, sections.plans]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !text.is_empty() {
                        for p in &record.composite_plans {
                            narrated_plans.insert(plan_key(p));
                        }
                        for a in &record.wsui.alerts {
                            alert_last_narrated.insert(alert_key(a), ply);
                        }
                        rows.insert(ply, text);
                    }
                }

                // Advance the standing story with the FULL picture.
                prev_alerts = cur_alerts;
                prev_themes = cur_themes;
                prev_phase = Some(record.phase);
                if fresh_verdict {
                    last_verdict_story = verdict_story;
                }
            }
            _ => {}
        }
    }

    conn.execute("DELETE FROM narrations WHERE game_id = ?1", [game_id])?;
    for (p, text) in &rows {
        conn.execute(
            "INSERT INTO narrations (game_id, ply, text) VALUES (?1, ?2, ?3)",
            rusqlite::params![game_id, p, text],
        )?;
    }
    Ok(rows.len() as u32)
}

/// Convert the first few UCI moves of an engine PV to SAN.
fn pv_to_san(board: &cozy_chess::Board, pv: &[String]) -> Vec<String> {
    let mut b2 = board.clone();
    let mut sans = Vec::new();
    for uci in pv.iter().take(3) {
        let Ok(mv) = uci.parse::<cozy_chess::Move>() else {
            break;
        };
        if !b2.is_legal(mv) {
            break;
        }
        sans.push(crate::san::format_san(&b2, mv));
        b2.play(mv);
    }
    sans
}

/// Fetch a game's narrations keyed by ply (for export and the UI).
pub fn narrations(conn: &Connection, game_id: i64) -> anyhow::Result<HashMap<u32, String>> {
    let mut stmt = conn.prepare("SELECT ply, text FROM narrations WHERE game_id = ?1")?;
    let rows = stmt.query_map([game_id], |r| {
        Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
