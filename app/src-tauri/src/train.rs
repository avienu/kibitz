//! Repertoire Trainer IPC commands (ROADMAP Phase 5, opening SRS).
//!
//! Thin camelCase wrappers over `kibitz_db::repertoire`: due counts for
//! the Train tab badge, the due queue, FSRS grading, and adding a line to
//! a repertoire from the game view. All scheduling math lives in the BSD
//! `kibitz-srs` crate; the engine is never involved (CLAUDE.md #6).

use kibitz_srs::{Grade, Scheduler};
use serde::Serialize;
use tauri::State;

use crate::browse::{with_conn, DbState};

fn parse_color(color: &str) -> Result<kibitz_profile::Color, String> {
    match color {
        "white" => Ok(kibitz_profile::Color::White),
        "black" => Ok(kibitz_profile::Color::Black),
        other => Err(format!(
            "color must be \"white\" or \"black\", got {other:?}"
        )),
    }
}

fn parse_grade(grade: &str) -> Result<Grade, String> {
    match grade {
        "again" => Ok(Grade::Again),
        "hard" => Ok(Grade::Hard),
        "good" => Ok(Grade::Good),
        "easy" => Ok(Grade::Easy),
        other => Err(format!("grade must be again|hard|good|easy, got {other:?}")),
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainCountsDto {
    pub due: u32,
    pub total: u32,
}

/// Due/total card counts per color (Train tab badge + queue header).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainSummaryDto {
    pub white: TrainCountsDto,
    pub black: TrainCountsDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueCardDto {
    pub card_id: i64,
    pub repertoire_name: String,
    pub fen: String,
    pub expected_san: String,
    pub expected_uci: String,
    pub ply: u32,
    pub line_prefix: String,
    pub due: String,
    pub is_new: bool,
    pub reps: u32,
    pub lapses: u32,
    /// Next interval per grade in raw days ({again, hard, good, easy});
    /// the UI formats ("<1 m", "2 d", ... — see lib/train.ts). Computed by
    /// the real scheduler, so it always equals what grading will set.
    pub previews: kibitz_db::repertoire::GradePreviews,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradedDto {
    pub card_id: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub interval_days: f64,
    pub due: String,
    pub reps: u32,
    pub lapses: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLineDto {
    pub repertoire: String,
    pub cards_added: u32,
    pub cards_existing: u32,
    /// Replace mode only: positions whose cards were rewritten to the
    /// line's move (the triage "adopt what you play" flow).
    pub cards_replaced: u32,
}

fn counts_impl(
    conn: &rusqlite::Connection,
    color: kibitz_profile::Color,
    now: &str,
) -> Result<TrainCountsDto, String> {
    let c = kibitz_db::repertoire::counts(conn, color, now).map_err(|e| e.to_string())?;
    Ok(TrainCountsDto {
        due: c.due,
        total: c.total,
    })
}

pub(crate) fn train_summary_impl(conn: &rusqlite::Connection) -> Result<TrainSummaryDto, String> {
    let now = kibitz_db::repertoire::now_utc(conn).map_err(|e| e.to_string())?;
    Ok(TrainSummaryDto {
        white: counts_impl(conn, kibitz_profile::Color::White, &now)?,
        black: counts_impl(conn, kibitz_profile::Color::Black, &now)?,
    })
}

pub(crate) fn train_queue_impl(
    conn: &rusqlite::Connection,
    color: &str,
    limit: u32,
) -> Result<Vec<DueCardDto>, String> {
    let color = parse_color(color)?;
    let now = kibitz_db::repertoire::now_utc(conn).map_err(|e| e.to_string())?;
    let scheduler = Scheduler::default();
    let cards = kibitz_db::repertoire::due_cards(conn, &scheduler, color, &now, limit)
        .map_err(|e| e.to_string())?;
    Ok(cards
        .into_iter()
        .map(|c| DueCardDto {
            card_id: c.card_id,
            repertoire_name: c.repertoire_name,
            fen: c.fen,
            expected_san: c.expected_san,
            expected_uci: c.expected_uci,
            ply: c.ply,
            line_prefix: c.line_prefix,
            due: c.due,
            is_new: c.is_new,
            reps: c.reps,
            lapses: c.lapses,
            previews: c.previews,
        })
        .collect())
}

pub(crate) fn train_grade_impl(
    conn: &rusqlite::Connection,
    card_id: i64,
    grade: &str,
) -> Result<GradedDto, String> {
    let grade = parse_grade(grade)?;
    let now = kibitz_db::repertoire::now_utc(conn).map_err(|e| e.to_string())?;
    let scheduler = Scheduler::default();
    let g = kibitz_db::repertoire::grade_card(conn, &scheduler, card_id, grade, &now)
        .map_err(|e| e.to_string())?;
    Ok(GradedDto {
        card_id: g.card_id,
        stability: g.memory.stability,
        difficulty: g.memory.difficulty,
        interval_days: g.interval_days,
        due: g.due,
        reps: g.reps,
        lapses: g.lapses,
    })
}

pub(crate) fn train_add_line_impl(
    conn: &rusqlite::Connection,
    color: &str,
    start_fen: Option<&str>,
    sans: &[String],
    name: Option<&str>,
    replace: bool,
) -> Result<AddLineDto, String> {
    let parsed = parse_color(color)?;
    let name = name.unwrap_or("main");
    let start: cozy_chess::Board = match start_fen {
        Some(fen) => fen.parse().map_err(|e| format!("bad start FEN: {e:?}"))?,
        None => cozy_chess::Board::default(),
    };
    let source = kibitz_db::import::SourceInfo {
        name: "Repertoire Trainer".into(),
        origin: "added from the game view".into(),
        license: "personal data".into(),
        kind: kibitz_db::import::SourceKind::Personal,
    };
    let rep_id = kibitz_db::repertoire::ensure_repertoire(conn, parsed, name, &source)
        .map_err(|e| e.to_string())?;
    let now = kibitz_db::repertoire::now_utc(conn).map_err(|e| e.to_string())?;
    let st = if replace {
        // Triage "adopt what you play": conflicting cards along the line
        // are rewritten to the played move instead of silently kept.
        kibitz_db::repertoire::add_line_replacing(conn, rep_id, parsed, &start, sans, &now)
    } else {
        kibitz_db::repertoire::add_line(conn, rep_id, parsed, &start, sans, &now)
    }
    .map_err(|e| e.to_string())?;
    Ok(AddLineDto {
        repertoire: format!("{name} ({color})"),
        cards_added: st.cards_added,
        cards_existing: st.cards_existing,
        cards_replaced: st.cards_replaced,
    })
}

/// Due/total counts for both colors (Train tab badge).
#[tauri::command]
pub async fn train_summary(state: State<'_, DbState>) -> Result<TrainSummaryDto, String> {
    with_conn(&state, train_summary_impl)
}

/// Due cards for `color` ("white" | "black"), earliest due first.
#[tauri::command]
pub async fn train_queue(
    state: State<'_, DbState>,
    color: String,
    limit: Option<u32>,
) -> Result<Vec<DueCardDto>, String> {
    with_conn(&state, |conn| {
        train_queue_impl(conn, &color, limit.unwrap_or(100))
    })
}

/// Grade one card ("again" | "hard" | "good" | "easy") and reschedule it.
#[tauri::command]
pub async fn train_grade(
    state: State<'_, DbState>,
    card_id: i64,
    grade: String,
) -> Result<GradedDto, String> {
    with_conn(&state, |conn| train_grade_impl(conn, card_id, &grade))
}

/// Add a SAN line to the color's repertoire (created on first use).
/// `replace: true` rewrites cards that conflict with the line's moves —
/// the triage reality-check "adopt what you play" flow only.
#[tauri::command]
pub async fn train_add_line(
    state: State<'_, DbState>,
    color: String,
    start_fen: Option<String>,
    sans: Vec<String>,
    name: Option<String>,
    replace: Option<bool>,
) -> Result<AddLineDto, String> {
    with_conn(&state, |conn| {
        train_add_line_impl(
            conn,
            &color,
            start_fen.as_deref(),
            &sans,
            name.as_deref(),
            replace.unwrap_or(false),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    #[test]
    fn add_line_queue_grade_round_trip() {
        let (_dir, conn) = open_db();
        let sans: Vec<String> = ["e4", "e5", "Nf3", "Nc6", "Bb5"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let added = train_add_line_impl(&conn, "white", None, &sans, None, false).unwrap();
        assert_eq!(added.cards_added, 3, "e4, Nf3, Bb5");
        assert_eq!(added.repertoire, "main (white)");

        // Replace mode rewrites a conflicting card to the new line's move
        // (the triage "adopt what you play" flow); plain mode never does.
        let italian: Vec<String> = ["e4", "e5", "Bc4"].iter().map(|s| s.to_string()).collect();
        let plain = train_add_line_impl(&conn, "white", None, &italian, None, false).unwrap();
        assert_eq!((plain.cards_added, plain.cards_replaced), (0, 0));
        let replaced = train_add_line_impl(&conn, "white", None, &italian, None, true).unwrap();
        assert_eq!(
            (
                replaced.cards_added,
                replaced.cards_existing,
                replaced.cards_replaced
            ),
            (0, 1, 1),
            "the e4 card matches; the Nf3 card is rewritten to Bc4"
        );
        let json = serde_json::to_string(&replaced).unwrap();
        assert!(json.contains("\"cardsReplaced\":"), "{json}");
        // Put the Ruy card back for the rest of the round trip.
        let ruy: Vec<String> = ["e4", "e5", "Nf3"].iter().map(|s| s.to_string()).collect();
        train_add_line_impl(&conn, "white", None, &ruy, None, true).unwrap();

        let summary = train_summary_impl(&conn).unwrap();
        assert_eq!((summary.white.due, summary.white.total), (3, 3));
        assert_eq!(summary.black.total, 0);

        let queue = train_queue_impl(&conn, "white", 100).unwrap();
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0].expected_san, "e4");
        assert!(queue[0].is_new);

        // Wire shape: camelCase keys.
        let json = serde_json::to_string(&queue[0]).unwrap();
        for needle in [
            "\"cardId\":",
            "\"expectedSan\":",
            "\"linePrefix\":",
            "\"isNew\":",
            "\"previews\":",
            "\"good\":",
        ] {
            assert!(json.contains(needle), "missing {needle} in {json}");
        }

        // Grade previews come from the real scheduler and match grading.
        assert!((queue[0].previews.good - 3.7145).abs() < 1e-9);
        let graded = train_grade_impl(&conn, queue[0].card_id, "good").unwrap();
        assert!((queue[0].previews.good - graded.interval_days).abs() < 1e-12);
        assert!((graded.stability - 3.7145).abs() < 1e-6);
        assert_eq!(graded.reps, 1);
        let summary = train_summary_impl(&conn).unwrap();
        assert_eq!((summary.white.due, summary.white.total), (2, 3));

        // Bad inputs fail cleanly; the engine never spawned.
        assert!(train_queue_impl(&conn, "purple", 10).is_err());
        assert!(train_grade_impl(&conn, 999_999, "good").is_err());
        assert!(train_grade_impl(&conn, queue[0].card_id, "meh").is_err());
        assert_eq!(kibitz_db::engine::spawn_count(), 0);
    }
}
