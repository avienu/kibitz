//! Repertoire Trainer storage and review flow (ROADMAP Phase 5, opening
//! SRS; schema in migrations/0008_repertoire.sql).
//!
//! A repertoire is per-color. Lines are added from the game view or
//! imported from a PGN study; every mainline move of the training color
//! becomes one card `(position, expected move)` keyed by the ep-normalized
//! position hash (`crate::hash::position_hash` — NEVER raw
//! `Board::hash()`), so transpositions collapse onto a single card.
//! Scheduling is FSRS-4.5 via the BSD `silman-srs` crate; timestamps are
//! UTC `datetime('now')` strings and elapsed days are computed with
//! SQLite's `julianday`.
//!
//! The engine is never involved in any of this (CLAUDE.md #6).

use std::io::BufRead;

use cozy_chess::Board;
use rusqlite::{params, Connection, OptionalExtension};
use silman_profile::Color;
use silman_srs::{Grade, MemoryState, Scheduler};

use crate::import::SourceInfo;
use crate::pgn::PgnReader;

fn color_str(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}

/// UTC now in the database's timestamp format.
pub fn now_utc(conn: &Connection) -> anyhow::Result<String> {
    Ok(conn.query_row("SELECT datetime('now')", [], |r| r.get(0))?)
}

/// Find or create the repertoire `(color, name)`. The provenance source
/// row is only inserted when the repertoire is created.
pub fn ensure_repertoire(
    conn: &Connection,
    color: Color,
    name: &str,
    source: &SourceInfo,
) -> anyhow::Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM repertoires WHERE color = ?1 AND name = ?2",
            params![color_str(color), name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO sources (name, origin, license, kind) VALUES (?1, ?2, ?3, ?4)",
        params![
            source.name,
            source.origin,
            source.license,
            source.kind.as_str()
        ],
    )?;
    let source_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO repertoires (color, name, source_id) VALUES (?1, ?2, ?3)",
        params![color_str(color), name, source_id],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AddLineStats {
    /// New cards created (training-color moves not already covered).
    pub cards_added: u32,
    /// Training-color moves whose position already had a card.
    pub cards_existing: u32,
    /// Total plies walked.
    pub plies_walked: u32,
}

/// Add one line (SAN mainline from `start`) to a repertoire. Every move
/// the training color plays becomes a card prompting that move from the
/// position before it; opponent moves only extend the prompt context.
/// Positions that already have a card are left untouched (first move in
/// wins), so re-adding lines is idempotent.
pub fn add_line(
    conn: &Connection,
    repertoire_id: i64,
    color: Color,
    start: &Board,
    sans: &[String],
    now: &str,
) -> anyhow::Result<AddLineStats> {
    let mut board = start.clone();
    let mut stats = AddLineStats::default();
    let mut prefix = String::new();
    let mut move_no = start_move_no(start);
    let mut insert = conn.prepare_cached(
        "INSERT INTO repertoire_cards
             (repertoire_id, position_hash, fen, expected_san, expected_uci,
              ply, line_prefix, due)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (repertoire_id, position_hash) DO NOTHING",
    )?;
    for (ply, san) in sans.iter().enumerate() {
        let mv = crate::san::parse_san(&board, san)
            .map_err(|e| anyhow::anyhow!("ply {}: {e}", ply + 1))?;
        let to_move = match board.side_to_move() {
            cozy_chess::Color::White => Color::White,
            cozy_chess::Color::Black => Color::Black,
        };
        if to_move == color {
            let hash = crate::hash::position_hash(&board) as i64;
            let changed = insert.execute(params![
                repertoire_id,
                hash,
                board.to_string(),
                san,
                mv.to_string(),
                ply as i64,
                prefix,
                now,
            ])?;
            if changed > 0 {
                stats.cards_added += 1;
            } else {
                stats.cards_existing += 1;
            }
        }
        // Extend the numbered-SAN prompt with the move just examined.
        let number = match board.side_to_move() {
            cozy_chess::Color::White => format!("{move_no}. "),
            cozy_chess::Color::Black if prefix.is_empty() => format!("{move_no}... "),
            cozy_chess::Color::Black => String::new(),
        };
        if !prefix.is_empty() {
            prefix.push(' ');
        }
        prefix.push_str(&number);
        prefix.push_str(san);
        if board.side_to_move() == cozy_chess::Color::Black {
            move_no += 1;
        }
        board.play(mv);
        stats.plies_walked += 1;
    }
    Ok(stats)
}

/// Fullmove number of the start position (1 for the standard start).
fn start_move_no(start: &Board) -> usize {
    // FEN field 6 is the fullmove number.
    start
        .to_string()
        .split_ascii_whitespace()
        .nth(5)
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

#[derive(Debug, Default)]
pub struct RepertoireImportStats {
    pub games_read: u32,
    pub games_failed: u32,
    pub line: AddLineStats,
    /// First few failure descriptions, for reporting.
    pub failures: Vec<String>,
}

/// Import a PGN file as a repertoire for `color`: the mainline of every
/// game becomes a line (variations are currently ignored). Games with a
/// custom start position train from that position.
pub fn import_pgn_repertoire<R: BufRead>(
    conn: &Connection,
    color: Color,
    name: &str,
    source: &SourceInfo,
    reader: R,
) -> anyhow::Result<RepertoireImportStats> {
    let repertoire_id = ensure_repertoire(conn, color, name, source)?;
    let now = now_utc(conn)?;
    let mut stats = RepertoireImportStats::default();
    conn.execute_batch("BEGIN")?;
    for item in PgnReader::new(reader) {
        let added = item.map_err(anyhow::Error::from).and_then(|raw| {
            let start: Board = match raw.tag("FEN") {
                Some(fen) => fen
                    .parse()
                    .map_err(|e| anyhow::anyhow!("line {}: bad FEN: {e:?}", raw.start_line))?,
                None => Board::default(),
            };
            add_line(
                conn,
                repertoire_id,
                color,
                &start,
                &raw.mainline_sans(),
                &now,
            )
            .map_err(|e| anyhow::anyhow!("game at line {}: {e}", raw.start_line))
        });
        match added {
            Ok(line) => {
                stats.games_read += 1;
                stats.line.cards_added += line.cards_added;
                stats.line.cards_existing += line.cards_existing;
                stats.line.plies_walked += line.plies_walked;
            }
            Err(e) => {
                stats.games_failed += 1;
                if stats.failures.len() < 10 {
                    stats.failures.push(e.to_string());
                }
            }
        }
    }
    conn.execute_batch("COMMIT")?;
    Ok(stats)
}

/// Next-interval preview per grade, in raw (unformatted) days — the UI
/// formats ("<1 m", "2 d", ...). Computed with the REAL scheduler on the
/// card's current memory state and elapsed time, exactly as [`grade_card`]
/// will, so a preview always equals what grading then does.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct GradePreviews {
    pub again: f64,
    pub hard: f64,
    pub good: f64,
    pub easy: f64,
}

/// One due card, ready to present.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DueCard {
    pub card_id: i64,
    pub repertoire_id: i64,
    pub repertoire_name: String,
    pub fen: String,
    pub expected_san: String,
    pub expected_uci: String,
    pub ply: u32,
    pub line_prefix: String,
    pub due: String,
    /// True until the card's first review.
    pub is_new: bool,
    pub reps: u32,
    pub lapses: u32,
    /// Next interval per grade (days) for the grade-row buttons.
    pub previews: GradePreviews,
}

/// Cards of `color` due at `now`, earliest due first (new cards are due at
/// creation time), then shallower positions first as a tiebreak.
pub fn due_cards(
    conn: &Connection,
    scheduler: &Scheduler,
    color: Color,
    now: &str,
    limit: u32,
) -> anyhow::Result<Vec<DueCard>> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.repertoire_id, r.name, c.fen, c.expected_san,
                c.expected_uci, c.ply, c.line_prefix, c.due, c.reps, c.lapses,
                c.stability, c.difficulty,
                -- Elapsed days since the last review: the same julianday
                -- computation grade_card uses, so previews match grading.
                COALESCE(MAX(julianday(?2) - julianday(c.last_review), 0.0), 0.0)
         FROM repertoire_cards c
         JOIN repertoires r ON r.id = c.repertoire_id
         WHERE r.color = ?1 AND c.due <= ?2
         ORDER BY c.due, c.ply, c.id
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![color_str(color), now, limit], |r| {
        let reps: u32 = r.get(9)?;
        let stability: Option<f64> = r.get(11)?;
        let difficulty: Option<f64> = r.get(12)?;
        let elapsed_days: f64 = r.get(13)?;
        let state = match (stability, difficulty) {
            (Some(s), Some(d)) => Some(MemoryState {
                stability: s,
                difficulty: d,
            }),
            _ => None,
        };
        let preview = |g: Grade| scheduler.next(state, elapsed_days, g).interval_days;
        Ok(DueCard {
            card_id: r.get(0)?,
            repertoire_id: r.get(1)?,
            repertoire_name: r.get(2)?,
            fen: r.get(3)?,
            expected_san: r.get(4)?,
            expected_uci: r.get(5)?,
            ply: r.get(6)?,
            line_prefix: r.get(7)?,
            due: r.get(8)?,
            is_new: reps == 0,
            reps,
            lapses: r.get(10)?,
            previews: GradePreviews {
                again: preview(Grade::Again),
                hard: preview(Grade::Hard),
                good: preview(Grade::Good),
                easy: preview(Grade::Easy),
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Per-color card counts for the Train tab badge and queue header.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct ColorCounts {
    pub due: u32,
    pub total: u32,
}

pub fn counts(conn: &Connection, color: Color, now: &str) -> anyhow::Result<ColorCounts> {
    let (due, total) = conn.query_row(
        "SELECT COALESCE(SUM(c.due <= ?2), 0), COUNT(*)
         FROM repertoire_cards c
         JOIN repertoires r ON r.id = c.repertoire_id
         WHERE r.color = ?1",
        params![color_str(color), now],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(ColorCounts { due, total })
}

/// Result of grading one card.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Graded {
    pub card_id: i64,
    pub memory: MemoryState,
    pub interval_days: f64,
    pub due: String,
    pub reps: u32,
    pub lapses: u32,
}

/// Apply one FSRS review to a card at time `now` and persist the new
/// state plus a `repertoire_reviews` log row. The elapsed time feeding
/// FSRS is the number of days since the previous review (0 for the first).
pub fn grade_card(
    conn: &Connection,
    scheduler: &Scheduler,
    card_id: i64,
    grade: Grade,
    now: &str,
) -> anyhow::Result<Graded> {
    let (stability, difficulty, last_review, reps, lapses): (
        Option<f64>,
        Option<f64>,
        Option<String>,
        u32,
        u32,
    ) = conn
        .query_row(
            "SELECT stability, difficulty, last_review, reps, lapses
             FROM repertoire_cards WHERE id = ?1",
            [card_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("no such card: {card_id}"))?;

    let state = match (stability, difficulty) {
        (Some(s), Some(d)) => Some(MemoryState {
            stability: s,
            difficulty: d,
        }),
        _ => None,
    };
    let elapsed_days: f64 = match &last_review {
        Some(prev) => conn.query_row(
            "SELECT MAX(julianday(?1) - julianday(?2), 0.0)",
            params![now, prev],
            |r| r.get(0),
        )?,
        None => 0.0,
    };
    let review = scheduler.next(state, elapsed_days, grade);
    let interval_secs = (review.interval_days * 86_400.0).round() as i64;
    let due: String = conn.query_row(
        "SELECT datetime(?1, '+' || ?2 || ' seconds')",
        params![now, interval_secs],
        |r| r.get(0),
    )?;
    // A lapse is forgetting a previously learned card; a first-review
    // Again just means the new card starts hard.
    let new_lapses = lapses + u32::from(grade == Grade::Again && reps > 0);
    let new_reps = reps + 1;
    conn.execute(
        "UPDATE repertoire_cards
         SET stability = ?1, difficulty = ?2, due = ?3, reps = ?4,
             lapses = ?5, last_review = ?6
         WHERE id = ?7",
        params![
            review.memory.stability,
            review.memory.difficulty,
            due,
            new_reps,
            new_lapses,
            now,
            card_id
        ],
    )?;
    conn.execute(
        "INSERT INTO repertoire_reviews
             (card_id, reviewed_at, grade, elapsed_days, stability,
              difficulty, interval_days)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            card_id,
            now,
            grade.value() as i64,
            elapsed_days,
            review.memory.stability,
            review.memory.difficulty,
            review.interval_days
        ],
    )?;
    Ok(Graded {
        card_id,
        memory: review.memory,
        interval_days: review.interval_days,
        due,
        reps: new_reps,
        lapses: new_lapses,
    })
}
