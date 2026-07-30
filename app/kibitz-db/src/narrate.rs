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

/// A completed engine verdict for one mainline ply: a wsui-confirm
/// alert grading, a suggest-verify suggestion review, or (merged) both.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// The wsui-confirm grading of the fired alert. `None` for a ply
    /// covered only by a suggest-verify row — a suggestion review carries
    /// no confirm status, and must never masquerade as one.
    pub status: Option<EngineCheckStatus>,
    pub pv_uci: Vec<String>,
    pub score_delta_cp: Option<i32>,
    pub mate_in: Option<i32>,
    pub nodes: u64,
    /// Engine-cleared static candidate moves for this position (run 11):
    /// the wsui-confirm or suggest-verify job's cursory suggestion
    /// review. `Some` even when empty (the engine ran and refuted
    /// everything); `None` when the job predates the review or had
    /// nothing to verify — then the static veto governs.
    pub cleared_suggestions: Option<Vec<String>>,
}

/// Load every completed verdict for a game, keyed by mainline ply:
/// wsui-confirm rows carry the alert grading (and, since run 11, a
/// suggestion review at fired plies); suggest-verify rows carry only the
/// suggestion review at quiet closing-eligible plies. The two purposes
/// target disjoint plies by construction; if both ever land on one ply
/// the confirm verdict wins and the first available cleared list is kept.
pub fn load_verdicts(conn: &Connection, game_id: i64) -> anyhow::Result<HashMap<u32, Verdict>> {
    let mut out: HashMap<u32, Verdict> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT purpose, payload, result FROM jobs
         WHERE purpose IN ('wsui-confirm', 'suggest-verify')
           AND status = 'done' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (purpose, payload, result) = row?;
        let (row_game, row_ply) = if purpose == "suggest-verify" {
            let p: crate::jobs::SuggestVerifyPayload = serde_json::from_str(&payload)?;
            (Some(p.game_id), Some(p.ply))
        } else {
            let p: crate::jobs::EnginePayload = serde_json::from_str(&payload)?;
            (p.game_id, p.ply)
        };
        if row_game != Some(game_id) {
            continue;
        }
        let Some(ply) = row_ply else { continue };
        let v: serde_json::Value = serde_json::from_str(&result)?;
        let cleared_suggestions = v["cleared_suggestions"].as_array().map(|list| {
            list.iter()
                .filter_map(|u| u.as_str().map(str::to_string))
                .collect()
        });
        if purpose == "suggest-verify" {
            // No confirm status to contribute: merge only the cleared
            // list, never displacing an existing confirm verdict.
            let entry = out.entry(ply).or_insert_with(|| Verdict {
                status: None,
                pv_uci: Vec::new(),
                score_delta_cp: None,
                mate_in: None,
                nodes: 0,
                cleared_suggestions: None,
            });
            if entry.cleared_suggestions.is_none() {
                entry.cleared_suggestions = cleared_suggestions;
            }
            continue;
        }
        let status = match v["status"].as_str() {
            Some("confirmed") => EngineCheckStatus::Confirmed,
            Some("refuted") => EngineCheckStatus::Refuted,
            _ => EngineCheckStatus::UnclearAtBudget,
        };
        let mate_in = v["mate_for_beneficiary"].as_i64().map(|m| m as i32);
        let mut verdict = Verdict {
            status: Some(status),
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
            cleared_suggestions,
        };
        if verdict.cleared_suggestions.is_none() {
            if let Some(prev) = out.get(&ply) {
                verdict.cleared_suggestions = prev.cleared_suggestions.clone();
            }
        }
        out.insert(ply, verdict);
    }
    Ok(out)
}

/// A mainline capture ply: the move takes a piece, or is an en passant
/// capture. Shared by the narrator's closing gate and annotate's
/// suggest-verify eligibility — the two must agree on what "capture ply"
/// means (mid-exchange the only honest advice is to finish the exchange,
/// so no closing renders and no review is worth enqueueing).
pub(crate) fn is_capture_ply(board_before: &cozy_chess::Board, mv: cozy_chess::Move) -> bool {
    board_before.colors(!board_before.side_to_move()).has(mv.to)
        || (board_before.piece_on(mv.from) == Some(cozy_chess::Piece::Pawn)
            && mv.from.file() != mv.to.file()
            && board_before.piece_on(mv.to).is_none())
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
    let mut key = format!("{:?}:{:?}", i.kind, i.favors);
    // Development-prior stories (run 11): a queen sortie or a wandering
    // piece appearing IS news even while the development theme itself is
    // already standing — fold the misplay flags into the identity so the
    // delta narrator re-tells the theme exactly when the story changes.
    if i.kind == kibitz_core::record::ImbalanceKind::Development {
        if i.evidence.keys().any(|k| k.starts_with("queen_sortie")) {
            key.push_str(":sortie");
        }
        if i.evidence.keys().any(|k| k.starts_with("wanderer")) {
            key.push_str(":wander");
        }
    }
    key
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

    // Book awareness (run 11): while the mainline is still inside the
    // bundled openings book, the development prior stays quiet — theory
    // has walked that road — and one book line is narrated at most once.
    // The book state latches: the first out-of-book position ends it for
    // good (transpositions back into named theory don't restart it).
    let theory = crate::fingerprint::theory_set(conn)?;
    let mut left_book = false;
    let mut book_told = false;
    let mut mainline: Vec<cozy_chess::Move> = Vec::new();

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
                mainline.push(*mv);
                ply += 1;
                if board.status() != cozy_chess::GameStatus::Ongoing {
                    // The game is over; narrating hanging pieces in a
                    // checkmate diagram helps nobody.
                    break;
                }

                let in_book = !left_book && theory.contains(&crate::hash::position_hash(&board));
                if !in_book {
                    left_book = true;
                }

                let mut record = kibitz_core::analyze(&board);
                // Development prior (run 11): fed with the full move
                // history — but only once the game leaves the book. The
                // first out-of-book moment is where this voice may start.
                if !in_book {
                    let report = kibitz_core::development::track(&start, &mainline);
                    kibitz_core::development::augment(&mut record, &report);
                }
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

                // Merge this ply's engine verdict, if any. A ply covered
                // only by a suggest-verify row has no confirm status —
                // its cleared list feeds the closing below, but it never
                // grades (or drops) an alert.
                let mut verdict_status = None;
                if let Some(v) = verdicts.get(&ply) {
                    if let Some(status) = v.status {
                        verdict_status = Some(status);
                        match status {
                            EngineCheckStatus::Refuted => {
                                // The tactic does not work: drop the lead alert.
                                if !record.wsui.alerts.is_empty() {
                                    record.wsui.alerts.remove(0);
                                }
                            }
                            _ => {
                                if let Some(top) = record.wsui.alerts.first_mut() {
                                    top.engine_check = Some(EngineCheck {
                                        status,
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
                // a phase boundary restate everything still standing. A
                // story FLAG dropping off (the wanderer was traded, the
                // queen went home) is not news: a previous key that
                // extends the current one already told this theme.
                record.imbalances.retain(|i| {
                    let key = theme_key(i);
                    let told = prev_themes.get(&key) == Some(&i.magnitude)
                        || prev_themes
                            .iter()
                            .any(|(k, m)| *m == i.magnitude && k.starts_with(&format!("{key}:")));
                    i.magnitude >= Magnitude::Clear && (phase_boundary || !told)
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
                let mut text = String::new();
                if has_content && rows.len() < max_comments as usize {
                    let mut sections = kibitz_verbalize::verbalize_sections_voiced(&record, voice);
                    // Candidate-move closing (run 10): one "what to play"
                    // sentence at plies where plans are narrated — but
                    // never on a capture ply, where the only honest
                    // advice is to finish the exchange in progress.
                    let capture_ply = is_capture_ply(&board_before, *mv);
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
                    text = [sections.tactics, sections.imbalances, sections.plans]
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
                    }
                }
                // The single quiet book line (run 11), told once per
                // game at the first in-book ply — never per ply.
                if in_book && !book_told && rows.len() < max_comments as usize {
                    book_told = true;
                    let line = kibitz_verbalize::book_line(voice);
                    text = if text.is_empty() {
                        line
                    } else {
                        format!("{line} {text}")
                    };
                }
                if !text.is_empty() {
                    rows.insert(ply, text);
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
