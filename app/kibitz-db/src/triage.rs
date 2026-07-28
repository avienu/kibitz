//! Opening triage (run 10): walk the user's recent games against their
//! repertoire cards and say exactly where the opening play needs work.
//!
//! Three classes of triage point, detected per game at the FIRST moment
//! the game leaves the user's book:
//!
//! - **Deviation** — the user had a card for the position but played a
//!   different move. Their book covered the spot; they forgot it.
//! - **Gap** — while the user was in book, the opponent played a move
//!   whose resulting position has no card, although at least one OTHER
//!   opponent reply from the same position IS covered. The book has a
//!   hole for this specific opponent move.
//! - **Frontier** — user and opponent stayed in book until it simply
//!   ended: after the user's last carded move, NO opponent reply leads to
//!   any carded position. The recorded position is that end-of-book
//!   position (opponent to move) — the natural spot to extend.
//!
//! Aggregation is by ep-normalized position hash (`crate::hash`), so the
//! same triage point reached via transposition or by N different games
//! collapses into one ranked item. Ranking: game count desc, then
//! earliest ply, then FEN (deterministic).
//!
//! Everything in this module is a static database walk — no engine
//! (CLAUDE.md #6). Book EXTENSIONS (the engine-proposed candidate lines
//! for a gap/frontier) are only ever produced by the job queue
//! ('book-extension' jobs, explicit user request); this module just
//! stores and reads their results (`book_extensions`, migration 0013).

use std::collections::HashMap;

use cozy_chess::{Board, Color as CozyColor, Move};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::hash::position_hash;
use crate::movebin::decode_game;
use crate::san::format_san;

// ---------------------------------------------------------------------------
// classification (pure over a card-lookup closure)
// ---------------------------------------------------------------------------

/// What one game contributes to the triage, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    Deviation {
        /// Position the user moved from (they had a card here).
        fen: String,
        /// 1-based mainline ply of the user's off-book move.
        ply: u32,
        expected_san: String,
        played_san: String,
        /// Numbered SAN of the moves leading to the position.
        line: String,
    },
    Gap {
        /// Position AFTER the opponent's uncovered move (user to move —
        /// the position the user needs an answer for).
        fen: String,
        /// 1-based ply of the opponent's uncovered move.
        ply: u32,
        opponent_san: String,
        /// Numbered SAN through the opponent's move.
        line: String,
    },
    Frontier {
        /// Position after the user's last carded move (opponent to move —
        /// where the book ends).
        fen: String,
        /// 1-based ply of that last carded move.
        ply: u32,
        /// Numbered SAN through that move.
        line: String,
    },
}

/// Append `san` to a numbered-SAN line (standard-start move numbering).
fn push_numbered(prefix: &mut String, to_move: CozyColor, move_no: u32, san: &str) {
    let number = match to_move {
        CozyColor::White => format!("{move_no}. "),
        CozyColor::Black if prefix.is_empty() => format!("{move_no}... "),
        CozyColor::Black => String::new(),
    };
    if !prefix.is_empty() {
        prefix.push(' ');
    }
    prefix.push_str(&number);
    prefix.push_str(san);
}

/// Does any opponent reply OTHER than `played` from `before` lead to a
/// carded position? (The gap-vs-frontier discriminator.)
fn sibling_covered<F>(before: &Board, played: Move, card_at: &mut F) -> anyhow::Result<bool>
where
    F: FnMut(u64) -> anyhow::Result<Option<(String, String)>>,
{
    let mut replies = Vec::new();
    before.generate_moves(|pm| {
        replies.extend(pm);
        false
    });
    for r in replies {
        if r == played {
            continue;
        }
        let mut b = before.clone();
        b.play(r);
        if card_at(position_hash(&b))?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Classify one standard-start mainline against the user's book for the
/// color they played. `card_at(position_hash)` returns the repertoire
/// card `(expected_san, expected_uci)` covering a position, if any —
/// the exact first-card-wins lookup `repertoire::game_marks` uses.
///
/// Returns the game's FIRST triage point (everything after it is out of
/// book), or `None` when the game stayed in book to the end of the
/// window, or never was in the user's book at all.
pub fn classify_game<F>(
    is_white: bool,
    moves: &[Move],
    max_plies: usize,
    mut card_at: F,
) -> anyhow::Result<Option<GameEvent>>
where
    F: FnMut(u64) -> anyhow::Result<Option<(String, String)>>,
{
    let user = if is_white {
        CozyColor::White
    } else {
        CozyColor::Black
    };
    let mut board = Board::default();
    let mut prefix = String::new();
    let mut move_no = 1u32;
    let mut made_book_move = false;
    // Position after the user's last carded move: (fen, ply, line).
    let mut last_book: Option<(String, u32, String)> = None;
    // Opponent's most recent move: (position before it, move, ply, san).
    let mut prev_opp: Option<(Board, Move, u32, String)> = None;

    for (i, &mv) in moves.iter().take(max_plies).enumerate() {
        let ply = i as u32 + 1;
        let to_move = board.side_to_move();
        let san = format_san(&board, mv);
        let user_to_move = to_move == user;

        if user_to_move {
            match card_at(position_hash(&board))? {
                Some((expected_san, expected_uci)) => {
                    if mv.to_string() != expected_uci {
                        return Ok(Some(GameEvent::Deviation {
                            fen: board.to_string(),
                            ply,
                            expected_san,
                            played_san: san,
                            line: prefix.clone(),
                        }));
                    }
                    made_book_move = true;
                }
                None => {
                    // Out of book at the user's turn. A gap/frontier is
                    // only meaningful if the user WAS in book: they made a
                    // carded move earlier, or this is Black's very first
                    // reply (in book trivially at the game start).
                    let first_own_reply = !is_white && i == 1;
                    if let Some((opp_before, opp_mv, opp_ply, opp_san)) = prev_opp {
                        if made_book_move || first_own_reply {
                            if sibling_covered(&opp_before, opp_mv, &mut card_at)? {
                                return Ok(Some(GameEvent::Gap {
                                    fen: board.to_string(),
                                    ply: opp_ply,
                                    opponent_san: opp_san,
                                    line: prefix.clone(),
                                }));
                            }
                            if made_book_move {
                                let (fen, q_ply, line) = last_book.expect("book move was recorded");
                                return Ok(Some(GameEvent::Frontier {
                                    fen,
                                    ply: q_ply,
                                    line,
                                }));
                            }
                            // Black's first reply with no sibling covered:
                            // the black book doesn't start from the
                            // standard start — nothing to report.
                        }
                    }
                    // User White with no card at the start position: the
                    // white book doesn't start here — nothing to report.
                    return Ok(None);
                }
            }
        } else {
            prev_opp = Some((board.clone(), mv, ply, san.clone()));
        }

        push_numbered(&mut prefix, to_move, move_no, &san);
        if to_move == CozyColor::Black {
            move_no += 1;
        }
        board.play(mv);
        if user_to_move {
            // Reaching here on the user's turn means the move matched.
            last_book = Some((board.to_string(), ply, prefix.clone()));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// aggregation across games → the ranked report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TriageOptions {
    /// Most-recent games (per player identity, both colors) to walk.
    pub max_games: u32,
    /// Opening window: plies of each game examined.
    pub max_plies: usize,
    /// Example games kept per triage item (counting is uncapped).
    pub max_examples: usize,
}

impl Default for TriageOptions {
    fn default() -> Self {
        Self {
            max_games: 400,
            max_plies: 60,
            max_examples: 8,
        }
    }
}

/// One source game behind a triage item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageExample {
    pub game_id: i64,
    /// 1-based mainline ply of the triage point in THIS game (deep-link
    /// target: the position after that ply shows the point).
    pub ply: u32,
    pub white: String,
    pub black: String,
    pub date: String,
    /// Deviations only: what the user played in this game.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_san: Option<String>,
}

/// One ranked triage point.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageItem {
    /// Position of the point (see [`GameEvent`] for which position each
    /// class records). Also the position a book extension analyses.
    pub fen: String,
    /// Earliest 1-based ply the point was reached at.
    pub ply: u32,
    /// Games that hit this exact point (the ranking key).
    pub games: u32,
    /// Numbered SAN of the earliest example's path to the point.
    pub line: String,
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    /// Deviations: the card's move.
    pub expected_san: Option<String>,
    /// Deviations: what the user played (earliest example).
    pub played_san: Option<String>,
    /// Gaps: the opponent's uncovered move.
    pub opponent_san: Option<String>,
    /// True when a completed book extension exists for `fen`.
    pub has_extension: bool,
    pub examples: Vec<TriageExample>,
}

/// Ranked lists for one repertoire color.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorTriage {
    /// "white" | "black" — the color the user played in these games.
    pub color: String,
    /// False when the color has no repertoire cards at all (its games are
    /// then skipped — an absent book has no gaps, honestly).
    pub has_cards: bool,
    /// Games of this color actually walked.
    pub games_scanned: u32,
    pub deviations: Vec<TriageItem>,
    pub gaps: Vec<TriageItem>,
    pub frontiers: Vec<TriageItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageReport {
    pub player: String,
    pub white: ColorTriage,
    pub black: ColorTriage,
}

/// Working aggregate per (class, position hash).
struct Agg {
    fen: String,
    min_ply: u32,
    count: u32,
    line: String,
    expected_san: Option<String>,
    played_san: Option<String>,
    opponent_san: Option<String>,
    examples: Vec<TriageExample>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Class {
    Deviation,
    Gap,
    Frontier,
}

/// Build the triage report for `player` (identity-resolved: lexical name
/// variants and declared aliases all count as the user — run 8.5).
pub fn triage_report(
    conn: &Connection,
    player: &str,
    opts: &TriageOptions,
) -> anyhow::Result<TriageReport> {
    let ids = crate::identity::resolve_identity_ids(conn, player)?;
    if ids.is_empty() {
        anyhow::bail!("no player named {player:?} in this database");
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");

    // Openings table for ECO naming of the items (idempotent).
    crate::eco::ensure_openings(conn)?;

    let has_cards = |color: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM repertoire_cards c
                 JOIN repertoires r ON r.id = c.repertoire_id
                 WHERE r.color = ?1)",
            [color],
            |r| r.get(0),
        )
    };
    let white_has_cards = has_cards("white")?;
    let black_has_cards = has_cards("black")?;

    // First-card-wins lookup, identical to repertoire::game_marks.
    let mut lookup = conn.prepare_cached(
        "SELECT c.expected_san, c.expected_uci FROM repertoire_cards c
         JOIN repertoires r ON r.id = c.repertoire_id
         WHERE r.color = ?1 AND c.position_hash = ?2
         ORDER BY c.id LIMIT 1",
    )?;

    // Recent games the user played, newest first. Custom-start games are
    // studies/fragments, not repertoire evidence (same rule as the
    // fingerprint) — skipped.
    let mut games_stmt = conn.prepare_cached(&format!(
        "SELECT g.white_id IN ({id_list}), g.id, g.movetext,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'),
                COALESCE(g.date, '')
         FROM games g
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         WHERE (g.white_id IN ({id_list}) OR g.black_id IN ({id_list}))
           AND g.start_fen IS NULL
         ORDER BY g.id DESC LIMIT ?1"
    ))?;
    let rows: Vec<(bool, i64, Vec<u8>, String, String, String)> = games_stmt
        .query_map([opts.max_games as i64], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let mut aggs: HashMap<(bool, Class, u64), Agg> = HashMap::new();
    let mut scanned = (0u32, 0u32); // (white, black) games walked

    for (is_white, game_id, movetext, white, black, date) in rows {
        let color_has_cards = if is_white {
            white_has_cards
        } else {
            black_has_cards
        };
        if !color_has_cards {
            continue;
        }
        let Ok(moves) = decode_game(&Board::default(), &movetext) else {
            continue;
        };
        if is_white {
            scanned.0 += 1;
        } else {
            scanned.1 += 1;
        }
        let color_str = if is_white { "white" } else { "black" };
        let event = classify_game(is_white, &moves, opts.max_plies, |hash| {
            Ok(lookup
                .query_row(params![color_str, hash as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?)
        })?;
        let Some(event) = event else { continue };

        let (class, fen, ply, line, expected_san, played_san, opponent_san) = match event {
            GameEvent::Deviation {
                fen,
                ply,
                expected_san,
                played_san,
                line,
            } => (
                Class::Deviation,
                fen,
                ply,
                line,
                Some(expected_san),
                Some(played_san),
                None,
            ),
            GameEvent::Gap {
                fen,
                ply,
                opponent_san,
                line,
            } => (Class::Gap, fen, ply, line, None, None, Some(opponent_san)),
            GameEvent::Frontier { fen, ply, line } => {
                (Class::Frontier, fen, ply, line, None, None, None)
            }
        };
        let board: Board = fen.parse().expect("fen came from a legal board");
        let key = (is_white, class, position_hash(&board));
        let example = TriageExample {
            game_id,
            ply,
            white,
            black,
            date,
            played_san: played_san.clone(),
        };
        let agg = aggs.entry(key).or_insert_with(|| Agg {
            fen,
            min_ply: ply,
            count: 0,
            line: line.clone(),
            expected_san,
            played_san,
            opponent_san,
            examples: Vec::new(),
        });
        agg.count += 1;
        if ply < agg.min_ply {
            agg.min_ply = ply;
            agg.line = line;
        }
        if agg.examples.len() < opts.max_examples {
            agg.examples.push(example);
        }
    }

    let build_color = |is_white: bool, has: bool, count: u32| -> anyhow::Result<ColorTriage> {
        let mut lists: [Vec<TriageItem>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for ((w, class, hash), agg) in &aggs {
            if *w != is_white {
                continue;
            }
            let (eco, opening_name) = match crate::eco::classify_hash(conn, *hash)? {
                Some((e, n)) => (Some(e), Some(n)),
                None => (None, None),
            };
            let has_extension = latest_book_extension(conn, &agg.fen)?.is_some();
            let item = TriageItem {
                fen: agg.fen.clone(),
                ply: agg.min_ply,
                games: agg.count,
                line: agg.line.clone(),
                eco,
                opening_name,
                expected_san: agg.expected_san.clone(),
                played_san: agg.played_san.clone(),
                opponent_san: agg.opponent_san.clone(),
                has_extension,
                examples: agg.examples.clone(),
            };
            let idx = match class {
                Class::Deviation => 0,
                Class::Gap => 1,
                Class::Frontier => 2,
            };
            lists[idx].push(item);
        }
        for list in &mut lists {
            list.sort_by(|a, b| {
                b.games
                    .cmp(&a.games)
                    .then(a.ply.cmp(&b.ply))
                    .then(a.fen.cmp(&b.fen))
            });
        }
        let [deviations, gaps, frontiers] = lists;
        Ok(ColorTriage {
            color: if is_white { "white" } else { "black" }.to_string(),
            has_cards: has,
            games_scanned: count,
            deviations,
            gaps,
            frontiers,
        })
    };

    let white = build_color(true, white_has_cards, scanned.0)?;
    let black = build_color(false, black_has_cards, scanned.1)?;
    Ok(TriageReport {
        player: player.to_string(),
        white,
        black,
    })
}

// ---------------------------------------------------------------------------
// book extensions: engine-proposed candidate lines (results storage)
// ---------------------------------------------------------------------------

/// Defaults for a book-extension request (the maintainer's "4 lines to
/// like ply 40 or 50": deep MultiPV analysis whose PVs extend the line
/// substantially past the book).
pub const EXTENSION_MULTIPV: u32 = 4;
pub const EXTENSION_DEPTH: u32 = 30;
/// Cap on stored plies per candidate line.
const MAX_LINE_PLIES: usize = 40;

/// One engine-proposed continuation from the analysed position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLine {
    /// SAN moves from the analysed position, alternating sides.
    pub sans: Vec<String>,
    /// Eval from the analysed position's side to move's POV.
    pub score_cp: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate: Option<i32>,
}

/// A stored book-extension result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookExtension {
    pub id: i64,
    pub fen: String,
    pub requested_at: String,
    pub engine: String,
    pub depth: u32,
    pub multipv: u32,
    pub lines: Vec<CandidateLine>,
}

/// Convert raw engine PVs (UCI, side-to-move POV) into SAN candidate
/// lines by replaying them from `fen`. Lines are capped at
/// [`MAX_LINE_PLIES`]; a PV that stops replaying cleanly is truncated at
/// the last legal move, and empty lines are dropped.
pub fn candidate_lines(
    fen: &str,
    raw: &[crate::engine::EngineLine],
) -> anyhow::Result<Vec<CandidateLine>> {
    let root: Board = fen
        .parse()
        .map_err(|e| anyhow::anyhow!("bad FEN {fen:?}: {e:?}"))?;
    Ok(raw
        .iter()
        .map(|l| {
            let mut board = root.clone();
            let mut sans = Vec::new();
            for uci in l.pv.iter().take(MAX_LINE_PLIES) {
                let Ok(mv) = crate::tactics::parse_uci(&board, uci) else {
                    break;
                };
                sans.push(format_san(&board, mv));
                board.play(mv);
            }
            CandidateLine {
                sans,
                score_cp: l.score_cp,
                mate: l.mate,
            }
        })
        .filter(|c| !c.sans.is_empty())
        .collect())
}

/// Persist one completed extension (called by the job worker).
pub fn store_book_extension(
    conn: &Connection,
    fen: &str,
    engine: &str,
    depth: u32,
    multipv: u32,
    lines: &[CandidateLine],
) -> anyhow::Result<i64> {
    let board: Board = fen
        .parse()
        .map_err(|e| anyhow::anyhow!("bad FEN {fen:?}: {e:?}"))?;
    conn.execute(
        "INSERT INTO book_extensions (position_hash, fen, engine, depth, multipv, lines)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            position_hash(&board) as i64,
            fen,
            engine,
            depth as i64,
            multipv as i64,
            serde_json::to_string(lines)?
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The most recent stored extension for `fen`'s position (transposition-
/// aware: lookup is by normalized position hash).
pub fn latest_book_extension(
    conn: &Connection,
    fen: &str,
) -> anyhow::Result<Option<BookExtension>> {
    let board: Board = fen
        .parse()
        .map_err(|e| anyhow::anyhow!("bad FEN {fen:?}: {e:?}"))?;
    let row: Option<(i64, String, String, String, i64, i64, String)> = conn
        .query_row(
            "SELECT id, fen, requested_at, engine, depth, multipv, lines
             FROM book_extensions WHERE position_hash = ?1
             ORDER BY id DESC LIMIT 1",
            [position_hash(&board) as i64],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((id, fen, requested_at, engine, depth, multipv, lines)) = row else {
        return Ok(None);
    };
    Ok(Some(BookExtension {
        id,
        fen,
        requested_at,
        engine,
        depth: depth as u32,
        multipv: multipv as u32,
        lines: serde_json::from_str(&lines)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{import_pgn, SourceInfo, SourceKind};
    use std::io::Cursor;

    fn source() -> SourceInfo {
        SourceInfo {
            name: "fixture".into(),
            origin: "unit test".into(),
            license: "public domain".into(),
            kind: SourceKind::Personal,
        }
    }

    fn open_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        (dir, conn)
    }

    /// Add a SAN line to a per-color repertoire (the trainAddLine path).
    fn add_rep_line(conn: &Connection, color: kibitz_profile::Color, sans: &[&str]) {
        let rep = crate::repertoire::ensure_repertoire(conn, color, "main", &source()).unwrap();
        let now = crate::repertoire::now_utc(conn).unwrap();
        let sans: Vec<String> = sans.iter().map(|s| s.to_string()).collect();
        crate::repertoire::add_line(conn, rep, color, &Board::default(), &sans, &now).unwrap();
    }

    fn play_sans(sans: &[&str]) -> (Board, Vec<Move>) {
        let mut board = Board::default();
        let mut moves = Vec::new();
        for san in sans {
            let mv = crate::san::parse_san(&board, san).unwrap();
            moves.push(mv);
            board.play(mv);
        }
        (board, moves)
    }

    /// Card table keyed by position hash for the pure classifier tests.
    fn card_map(lines: &[&[&str]], user_white: bool) -> HashMap<u64, (String, String)> {
        let mut cards = HashMap::new();
        for line in lines {
            let mut board = Board::default();
            for san in line.iter() {
                let mv = crate::san::parse_san(&board, san).unwrap();
                let user_turn = (board.side_to_move() == CozyColor::White) == user_white;
                if user_turn {
                    cards
                        .entry(position_hash(&board))
                        .or_insert_with(|| (san.to_string(), mv.to_string()));
                }
                board.play(mv);
            }
        }
        cards
    }

    fn classify_with_map(
        is_white: bool,
        game: &[&str],
        cards: &HashMap<u64, (String, String)>,
    ) -> Option<GameEvent> {
        let (_, moves) = play_sans(game);
        classify_game(is_white, &moves, 60, |h| Ok(cards.get(&h).cloned())).unwrap()
    }

    /// Ruy López fixture book: 1.e4 e5 2.Nf3 Nc6 3.Bb5 (Morphy's own
    /// weapon in the Opera-game era).
    const WHITE_BOOK: &[&[&str]] = &[&["e4", "e5", "Nf3", "Nc6", "Bb5"]];

    #[test]
    fn classify_finds_deviation_gap_and_frontier() {
        let cards = card_map(WHITE_BOOK, true);

        // Deviation: the book says 3.Bb5, the game played 3.Bc4.
        let ev = classify_with_map(true, &["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5"], &cards);
        match ev.expect("deviation detected") {
            GameEvent::Deviation {
                ply,
                expected_san,
                played_san,
                fen,
                line,
            } => {
                assert_eq!(ply, 5);
                assert_eq!(expected_san, "Bb5");
                assert_eq!(played_san, "Bc4");
                let (want, _) = play_sans(&["e4", "e5", "Nf3", "Nc6"]);
                assert_eq!(fen, want.to_string());
                assert_eq!(line, "1. e4 e5 2. Nf3 Nc6");
            }
            other => panic!("expected deviation, got {other:?}"),
        }

        // Gap: 1...c5 leaves book while 1...e5 is covered.
        let ev = classify_with_map(true, &["e4", "c5", "Nf3", "d6"], &cards);
        match ev.expect("gap detected") {
            GameEvent::Gap {
                ply,
                opponent_san,
                fen,
                line,
            } => {
                assert_eq!(ply, 2);
                assert_eq!(opponent_san, "c5");
                let (want, _) = play_sans(&["e4", "c5"]);
                assert_eq!(fen, want.to_string());
                assert_eq!(line, "1. e4 c5");
            }
            other => panic!("expected gap, got {other:?}"),
        }

        // Frontier: after 3.Bb5 the book simply ends — no black reply is
        // covered. The recorded position is the one after 3.Bb5.
        let ev = classify_with_map(
            true,
            &["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6"],
            &cards,
        );
        match ev.expect("frontier detected") {
            GameEvent::Frontier { ply, fen, line } => {
                assert_eq!(ply, 5);
                let (want, _) = play_sans(&["e4", "e5", "Nf3", "Nc6", "Bb5"]);
                assert_eq!(fen, want.to_string());
                assert_eq!(line, "1. e4 e5 2. Nf3 Nc6 3. Bb5");
            }
            other => panic!("expected frontier, got {other:?}"),
        }

        // Still in book at the end of a short game: nothing to report.
        assert_eq!(classify_with_map(true, &["e4", "e5", "Nf3"], &cards), None);

        // White book that doesn't cover the start position (custom-study
        // cards only): silent, never a fake triage point.
        let mut off_start = card_map(&[&["d4", "d5", "c4"]], true);
        off_start.remove(&position_hash(&Board::default()));
        assert_eq!(classify_with_map(true, &["e4", "e5"], &off_start), None);
    }

    #[test]
    fn classify_black_first_reply_gap_and_out_of_book_silence() {
        // Black book: 1.e4 c5 (and an answer to 2.Nf3).
        let cards = card_map(&[&["e4", "c5", "Nf3", "d6"]], false);

        // Opponent opens 1.d4 — uncovered, but 1.e4 IS covered: a gap at
        // the very first reply.
        let ev = classify_with_map(false, &["d4", "d5", "c4"], &cards);
        match ev.expect("first-reply gap") {
            GameEvent::Gap {
                ply, opponent_san, ..
            } => {
                assert_eq!(ply, 1);
                assert_eq!(opponent_san, "d4");
            }
            other => panic!("expected gap, got {other:?}"),
        }

        // Frontier as Black: 1.e4 c5 2.Nc3 — wait, 2.Nf3 d6 is covered, so
        // 2.Nc3 is a GAP; the frontier comes after 2.Nf3 d6 where the book
        // ends for every white 3rd move.
        let ev = classify_with_map(false, &["e4", "c5", "Nc3", "Nc6"], &cards);
        assert!(
            matches!(ev, Some(GameEvent::Gap { ply: 3, .. })),
            "sibling 2.Nf3 is covered: {ev:?}"
        );
        let ev = classify_with_map(false, &["e4", "c5", "Nf3", "d6", "d4", "cxd4"], &cards);
        match ev.expect("frontier detected") {
            GameEvent::Frontier { ply, fen, .. } => {
                assert_eq!(ply, 4, "book ends after 2...d6");
                let (want, _) = play_sans(&["e4", "c5", "Nf3", "d6"]);
                assert_eq!(fen, want.to_string());
            }
            other => panic!("expected frontier, got {other:?}"),
        }

        // A black book with no answer to anything from the start (cards
        // only deep in some other line): silent, not a fake gap.
        let empty: HashMap<u64, (String, String)> = HashMap::new();
        assert_eq!(classify_with_map(false, &["d4", "d5"], &empty), None);
    }

    /// Fixture games for the full report: the user under two lexically
    /// equivalent name forms, hitting one deviation, the 1...c5 gap twice,
    /// the 1...e6 gap once, and the Ruy frontier once.
    const GAMES: &str = r#"[Event "Club"]
[White "Tester, Ann"]
[Black "Kiebitz, Bea"]
[Result "1/2-1/2"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 1/2-1/2

[Event "Club"]
[White "Tester, Ann"]
[Black "Sizilianer, Carl"]
[Result "1-0"]

1. e4 c5 2. Nf3 d6 1-0

[Event "Club"]
[White "Tester, Ann"]
[Black "Sizilianer, Dora"]
[Result "0-1"]

1. e4 c5 2. Nc3 Nc6 0-1

[Event "Club"]
[White "Tester, Ann"]
[Black "Franzose, Emil"]
[Result "1-0"]

1. e4 e6 2. d4 d5 1-0

[Event "Online"]
[White "Ann Tester"]
[Black "Spanier, Fritz"]
[Result "1/2-1/2"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1/2-1/2
"#;

    #[test]
    fn report_aggregates_ranks_and_resolves_identity() {
        let (_dir, conn) = open_db();
        add_rep_line(
            &conn,
            kibitz_profile::Color::White,
            &["e4", "e5", "Nf3", "Nc6", "Bb5"],
        );
        let st = import_pgn(&conn, &source(), Cursor::new(GAMES)).unwrap();
        assert_eq!(st.games_imported, 5, "failures: {:?}", st.failures);

        let report = triage_report(&conn, "Tester, Ann", &TriageOptions::default()).unwrap();
        let w = &report.white;
        assert!(w.has_cards);
        assert_eq!(
            w.games_scanned, 5,
            "identity resolution merges 'Ann Tester' (run 8.5)"
        );

        // Deviation: 3.Bc4 instead of 3.Bb5.
        assert_eq!(w.deviations.len(), 1);
        let d = &w.deviations[0];
        assert_eq!((d.ply, d.games), (5, 1));
        assert_eq!(d.expected_san.as_deref(), Some("Bb5"));
        assert_eq!(d.played_san.as_deref(), Some("Bc4"));
        assert_eq!(d.line, "1. e4 e5 2. Nf3 Nc6");
        assert_eq!(d.examples.len(), 1);
        assert_eq!(d.examples[0].black, "Kiebitz, Bea");
        assert_eq!(d.examples[0].played_san.as_deref(), Some("Bc4"));

        // Gaps ranked by frequency: 1...c5 (2 games) before 1...e6 (1).
        assert_eq!(w.gaps.len(), 2);
        assert_eq!(w.gaps[0].opponent_san.as_deref(), Some("c5"));
        assert_eq!(w.gaps[0].games, 2);
        assert_eq!(w.gaps[0].examples.len(), 2);
        assert_eq!(w.gaps[1].opponent_san.as_deref(), Some("e6"));
        assert_eq!(w.gaps[1].games, 1);
        // The Sicilian gap position is book — named via the CC0 dataset.
        assert_eq!(w.gaps[0].eco.as_deref(), Some("B20"));

        // Frontier: after 3.Bb5.
        assert_eq!(w.frontiers.len(), 1);
        let f = &w.frontiers[0];
        assert_eq!((f.ply, f.games), (5, 1));
        let (want, _) = play_sans(&["e4", "e5", "Nf3", "Nc6", "Bb5"]);
        assert_eq!(f.fen, want.to_string());
        assert!(!f.has_extension, "no extension stored yet");

        // Black side: no cards → honest emptiness, games not scanned.
        let b = &report.black;
        assert!(!b.has_cards);
        assert_eq!(b.games_scanned, 0);
        assert!(b.deviations.is_empty() && b.gaps.is_empty() && b.frontiers.is_empty());

        // Wire shape: camelCase.
        let json = serde_json::to_string(&report).unwrap();
        for needle in [
            "\"hasCards\":",
            "\"gamesScanned\":",
            "\"expectedSan\":",
            "\"opponentSan\":",
            "\"hasExtension\":",
            "\"gameId\":",
            "\"openingName\":",
        ] {
            assert!(json.contains(needle), "missing {needle}");
        }

        // Unknown player fails cleanly; nothing spawned an engine.
        assert!(triage_report(&conn, "Nobody, At All", &TriageOptions::default()).is_err());
        assert_eq!(crate::engine::spawn_count(), 0);
    }

    #[test]
    fn extension_round_trips_and_flags_the_report() {
        let (_dir, conn) = open_db();
        add_rep_line(
            &conn,
            kibitz_profile::Color::White,
            &["e4", "e5", "Nf3", "Nc6", "Bb5"],
        );
        import_pgn(&conn, &source(), Cursor::new(GAMES)).unwrap();
        let report = triage_report(&conn, "Tester, Ann", &TriageOptions::default()).unwrap();
        let gap_fen = report.white.gaps[0].fen.clone();

        // Raw engine lines (UCI, incl. a castling move to exercise the
        // standard-form translation) → SAN candidate lines.
        let raw = vec![
            crate::engine::EngineLine {
                score_cp: 35,
                mate: None,
                pv: vec!["g1f3".into(), "d7d6".into(), "d2d4".into()],
            },
            crate::engine::EngineLine {
                score_cp: 20,
                mate: None,
                pv: vec!["b1c3".into(), "b8c6".into()],
            },
        ];
        let lines = candidate_lines(&gap_fen, &raw).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].sans, vec!["Nf3", "d6", "d4"]);
        assert_eq!(lines[0].score_cp, 35);

        // Castling translation: from the Ruy frontier position after
        // 3...a6 4.Ba4 Nf6, Stockfish's e1g1 must become O-O.
        let (castle_pos, _) = play_sans(&["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6"]);
        let castled = candidate_lines(
            &castle_pos.to_string(),
            &[crate::engine::EngineLine {
                score_cp: 30,
                mate: None,
                pv: vec!["e1g1".into(), "f8e7".into()],
            }],
        )
        .unwrap();
        assert_eq!(castled[0].sans, vec!["O-O", "Be7"]);

        // Store, read back, and see the report flag flip.
        let id = store_book_extension(&conn, &gap_fen, "Stockfish 17", 30, 4, &lines).unwrap();
        assert!(id > 0);
        let back = latest_book_extension(&conn, &gap_fen).unwrap().unwrap();
        assert_eq!(back.engine, "Stockfish 17");
        assert_eq!((back.depth, back.multipv), (30, 4));
        assert_eq!(back.lines, lines);
        assert!(!back.requested_at.is_empty());

        let report = triage_report(&conn, "Tester, Ann", &TriageOptions::default()).unwrap();
        assert!(report.white.gaps[0].has_extension);
        assert!(!report.white.gaps[1].has_extension);

        // Adoption path: the stored line becomes SRS cards from the gap
        // position (the trainAddLine reuse contract).
        let rep = crate::repertoire::ensure_repertoire(
            &conn,
            kibitz_profile::Color::White,
            "main",
            &source(),
        )
        .unwrap();
        let now = crate::repertoire::now_utc(&conn).unwrap();
        let start: Board = gap_fen.parse().unwrap();
        let st = crate::repertoire::add_line(
            &conn,
            rep,
            kibitz_profile::Color::White,
            &start,
            &back.lines[0].sans,
            &now,
        )
        .unwrap();
        assert_eq!(st.cards_added, 2, "Nf3 and d4 become cards");

        // The adopted gap no longer triages as a gap: the game now
        // follows book to the frontier of the new line.
        let report = triage_report(&conn, "Tester, Ann", &TriageOptions::default()).unwrap();
        assert_eq!(
            report.white.gaps.len(),
            1,
            "only the 1...e6 gap remains: {:?}",
            report.white.gaps
        );
        assert_eq!(report.white.gaps[0].opponent_san.as_deref(), Some("e6"));
        assert_eq!(crate::engine::spawn_count(), 0, "all of this is static");
    }
}
