//! Opening Lab (run 11): diagnose where an opening actually fails the
//! user from their OWN games, then ground book-move recommendations in
//! that evidence.
//!
//! The maintainer's framing: "I feel like I've struggled with my nimzo
//! for years never really making improvements … is there any way to find
//! how to pick the right book moves and train on those?" The Lab answers
//! with a reframe: find where the games actually DIE. Per cohort game it
//! computes the book-exit ply (walk against the bundled-openings theory
//! set), the eval at the exit, the FIRST significant error by the user
//! (stored evals only — a game without them is honestly "unanalyzed",
//! never guessed), and the middlegame structure tags (the profile's
//! shared classification). The aggregate verdict says whether the damage
//! happens in the book phase or later, and in which structures.
//!
//! Everything in this module is a static database walk — no engine
//! (CLAUDE.md #6). Engine candidates for a branch come exclusively from
//! the existing 'book-extension' job kind (explicit user request through
//! the job queue); this module only reads their stored results.

use std::collections::{HashMap, HashSet};

use cozy_chess::{Board, Color as CozyColor, Move};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::hash::position_hash;
use crate::movebin::decode_game;
use crate::san::format_san;
use crate::triage::push_numbered;

/// Opening window: plies of each game walked for book-exit detection and
/// branch-node collection (the bundled dataset is ~35 plies deep at most).
pub const LAB_MAX_BOOK_PLIES: usize = 40;
/// A user move is the game's FIRST significant error when it drops the
/// eval by at least this much (centipawns, user's point of view).
pub const FIRST_ERROR_CP: i32 = 120;
/// |eval| ≤ this (user POV, cp) counts as "roughly equal" at book exit.
pub const EQUALISH_CP: i32 = 50;
/// Mainline ply at which a game's middlegame structure is sampled
/// (clamped to the game length).
pub const STRUCTURE_SNAPSHOT_PLY: usize = 20;
/// Ranked branch nodes kept in a report.
const MAX_NODES: usize = 40;
/// Example games kept per branch node.
const MAX_NODE_EXAMPLES: usize = 6;
/// Homework rows kept (first-error positions in killer structures).
const MAX_HOMEWORK: usize = 40;
/// Killer structures considered for the homework list.
const KILLER_STRUCTURES: usize = 3;

// ---------------------------------------------------------------------------
// per-game diagnosis (pure over a theory set + eval map)
// ---------------------------------------------------------------------------

/// The game's first significant eval drop by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstError {
    /// 1-based mainline ply of the user's losing move.
    pub ply: u32,
    /// Eval drop in cp, user POV (≥ [`FIRST_ERROR_CP`]).
    pub swing_cp: i32,
    /// Eval before the move, user POV.
    pub before_cp: i32,
    /// Eval after the move, user POV.
    pub after_cp: i32,
    /// True when the error happened at or before the book exit (the
    /// opening phase); false = middlegame damage.
    pub in_book_phase: bool,
}

/// One user move made from an in-book position while the game was still
/// fully inside theory — the raw material of the branch table.
#[derive(Debug, Clone, PartialEq)]
pub struct BookMoveObs {
    /// Normalized hash of the position moved from (the node key).
    pub node_hash: u64,
    pub node_fen: String,
    /// 1-based ply of the user's move from this node.
    pub node_ply: u32,
    /// Numbered SAN of the moves leading to the node.
    pub line: String,
    /// The move the user played.
    pub san: String,
    /// True when the position AFTER the move is still in theory.
    pub after_in_book: bool,
    /// Stored eval after the move, user POV (when analyzed).
    pub eval_after_cp: Option<i32>,
    /// The opponent's actual reply (san, reply-still-in-book). Recorded
    /// only when the user's move itself stayed in book.
    pub reply: Option<(String, bool)>,
}

/// Everything one game contributes to the Lab.
#[derive(Debug, Clone, PartialEq)]
pub struct GameDiag {
    /// 1-based ply of the FIRST move producing an out-of-theory position;
    /// None = still in book through the opening window.
    pub exit_ply: Option<u32>,
    /// Eval at (or, fallback, just before) the exit, user POV.
    pub eval_at_exit_cp: Option<i32>,
    /// True when at least one of the user's moves had stored evals on
    /// both sides — the game can be error-checked. False = "unanalyzed":
    /// the Lab says so instead of guessing.
    pub analyzed: bool,
    pub first_error: Option<FirstError>,
    pub observations: Vec<BookMoveObs>,
}

/// Diagnose one standard-start mainline. `evals` maps 1-based ply →
/// stored eval in WHITE-POV centipawns (the [`crate::profile`] reading:
/// fresh preferred over legacy, POV already normalized); `theory` is the
/// set of normalized book-position hashes. The start position counts as
/// in-book by definition (the dataset only stores positions after ≥ 1
/// ply).
pub fn diagnose_game(
    is_white: bool,
    moves: &[Move],
    evals: &HashMap<u16, i32>,
    theory: &HashSet<u64>,
) -> GameDiag {
    let user = if is_white {
        CozyColor::White
    } else {
        CozyColor::Black
    };
    let pov = |cp: i32| if is_white { cp } else { -cp };

    let mut board = Board::default();
    let mut prefix = String::new();
    let mut move_no = 1u32;
    let mut exit_ply: Option<u32> = None;
    let mut observations: Vec<BookMoveObs> = Vec::new();
    // Index of the observation awaiting the opponent's reply.
    let mut awaiting_reply: Option<usize> = None;

    for (i, &mv) in moves.iter().take(LAB_MAX_BOOK_PLIES).enumerate() {
        if exit_ply.is_some() {
            break;
        }
        let ply = i as u32 + 1;
        let to_move = board.side_to_move();
        let san = format_san(&board, mv);
        let user_to_move = to_move == user;
        let node_fen = board.to_string();
        let node_hash = position_hash(&board);
        let line = prefix.clone();

        push_numbered(&mut prefix, to_move, move_no, &san);
        if to_move == CozyColor::Black {
            move_no += 1;
        }
        board.play(mv);
        let after_in_book = theory.contains(&position_hash(&board));

        if user_to_move {
            awaiting_reply = None;
            // Every position so far was in book (we break at the exit),
            // so this user-to-move position is a branch node.
            observations.push(BookMoveObs {
                node_hash,
                node_fen,
                node_ply: ply,
                line,
                san: san.clone(),
                after_in_book,
                eval_after_cp: evals.get(&(ply as u16)).map(|&c| pov(c)),
                reply: None,
            });
            if after_in_book {
                awaiting_reply = Some(observations.len() - 1);
            }
        } else if let Some(idx) = awaiting_reply.take() {
            observations[idx].reply = Some((san.clone(), after_in_book));
        }

        if !after_in_book {
            exit_ply = Some(ply);
        }
    }

    let eval_at_exit_cp = exit_ply.and_then(|e| {
        evals
            .get(&(e as u16))
            .or_else(|| evals.get(&((e - 1) as u16)))
            .map(|&c| pov(c))
    });

    // First significant error: scan the WHOLE game (middlegame damage is
    // the point), tolerant of missing evals.
    let mut analyzed = false;
    let mut first_error: Option<FirstError> = None;
    for i in 1..=moves.len() as u32 {
        let user_moved = (i % 2 == 1) == is_white;
        if !user_moved {
            continue;
        }
        let (Some(&before), Some(&after)) = (evals.get(&((i - 1) as u16)), evals.get(&(i as u16)))
        else {
            continue;
        };
        analyzed = true;
        let swing = pov(before) - pov(after);
        if swing >= FIRST_ERROR_CP && first_error.is_none() {
            first_error = Some(FirstError {
                ply: i,
                swing_cp: swing,
                before_cp: pov(before),
                after_cp: pov(after),
                in_book_phase: exit_ply.is_none_or(|e| i <= e),
            });
        }
    }

    GameDiag {
        exit_ply,
        eval_at_exit_cp,
        analyzed,
        first_error,
        observations,
    }
}

// ---------------------------------------------------------------------------
// cohort listing: the user's own openings, by family, with game counts
// ---------------------------------------------------------------------------

/// One pickable cohort: an opening family the user actually plays.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CohortRow {
    /// "white" | "black" — the color the user played these games.
    pub color: String,
    /// Family display name (the dataset name before any ":" variation
    /// suffix), e.g. "Nimzo-Indian Defense".
    pub family: String,
    /// Observed ECO range, e.g. "E20"–"E59".
    pub eco_min: String,
    pub eco_max: String,
    /// The exact ECO codes behind the family (the report's cohort key).
    pub ecos: Vec<String>,
    pub games: u32,
}

/// The user's openings (identity-resolved), grouped into families by the
/// bundled dataset's base names, games-heaviest first. Games without an
/// ECO tag are skipped — they have no opening identity to group under.
pub fn cohorts(conn: &Connection, player: &str) -> anyhow::Result<Vec<CohortRow>> {
    let ids = crate::identity::resolve_identity_ids(conn, player)?;
    if ids.is_empty() {
        anyhow::bail!("no player named {player:?} in this database");
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    crate::eco::ensure_openings(conn)?;

    let mut stmt = conn.prepare(&format!(
        "SELECT g.white_id IN ({id_list}), substr(g.eco, 1, 3), COUNT(*)
         FROM games g
         WHERE (g.white_id IN ({id_list}) OR g.black_id IN ({id_list}))
           AND g.start_fen IS NULL
           AND g.eco IS NOT NULL AND length(g.eco) >= 3
           AND g.result IN (1, 2, 3)
         GROUP BY 1, 2"
    ))?;
    let rows: Vec<(bool, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;

    // Family of an ECO code: the dataset's canonical name for the code,
    // truncated at the ":" variation separator.
    let mut family_of: HashMap<String, String> = HashMap::new();
    let mut groups: HashMap<(bool, String), (Vec<String>, u32)> = HashMap::new();
    for (is_white, eco, count) in rows {
        let family = match family_of.get(&eco) {
            Some(f) => f.clone(),
            None => {
                let f = crate::eco::name_for(conn, &eco)?
                    .map(|n| n.split(':').next().unwrap_or(&n).trim().to_string())
                    .unwrap_or_else(|| format!("ECO {eco}"));
                family_of.insert(eco.clone(), f.clone());
                f
            }
        };
        let entry = groups.entry((is_white, family)).or_default();
        if !entry.0.contains(&eco) {
            entry.0.push(eco);
        }
        entry.1 += count as u32;
    }

    let mut out: Vec<CohortRow> = groups
        .into_iter()
        .map(|((is_white, family), (mut ecos, games))| {
            ecos.sort();
            CohortRow {
                color: if is_white { "white" } else { "black" }.to_string(),
                eco_min: ecos.first().cloned().unwrap_or_default(),
                eco_max: ecos.last().cloned().unwrap_or_default(),
                family,
                ecos,
                games,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.games
            .cmp(&a.games)
            .then(a.color.cmp(&b.color))
            .then(a.family.cmp(&b.family))
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// the report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitStats {
    /// Games that left theory within the opening window.
    pub left_book: u32,
    /// Games still in theory at the window's edge (no exit observed).
    pub still_in_book: u32,
    pub median_exit_ply: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtExitStats {
    /// Games with a stored eval at (or just before) the exit.
    pub evaluated: u32,
    /// User POV within ±[`EQUALISH_CP`] cp.
    pub equal: u32,
    /// User POV better than +[`EQUALISH_CP`].
    pub better: u32,
    /// User POV worse than −[`EQUALISH_CP`] — already losing out of book.
    pub worse: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorStats {
    /// Games with enough stored evals to error-check at all.
    pub analyzed_games: u32,
    pub games_with_error: u32,
    /// First errors at or before the book exit.
    pub book_phase: u32,
    /// First errors after the book exit.
    pub middlegame: u32,
    /// Analyzed games with no ≥[`FIRST_ERROR_CP`] drop found.
    pub no_error_found: u32,
    pub median_error_ply: Option<u32>,
    /// Quartiles of the MIDDLEGAME first-error plies (the "damage happens
    /// at moves A–B" range).
    pub middlegame_p25_ply: Option<u32>,
    pub middlegame_p75_ply: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureStat {
    pub flag: String,
    pub games: u32,
    pub score_pct: f64,
    /// Expected points lost vs a 50% baseline: games × (0.5 − score),
    /// clamped at 0. The frequency-times-deficit ranking key.
    pub damage: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabReply {
    pub san: String,
    pub games: u32,
    /// True when the position after the reply is still in theory.
    pub in_book: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabMove {
    pub san: String,
    pub games: u32,
    pub score_pct: f64,
    /// Mean stored eval after the move, user POV cp (analyzed games only).
    pub avg_eval_cp: Option<i32>,
    /// Games contributing to `avg_eval_cp`.
    pub eval_games: u32,
    /// True when the position after the move is still in theory.
    pub in_book: bool,
    /// True when this exact move is the user's repertoire card here.
    pub in_rep: bool,
    /// games × (0.5 − score), clamped at 0 — the DAMAGE rank.
    pub damage: f64,
    /// Opponent replies actually faced after this move (in-book moves
    /// only), most frequent first. Coverage math lives on these.
    pub replies: Vec<LabReply>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabExample {
    pub game_id: i64,
    /// Ply of the user's move at the node in THIS game (deep-link target).
    pub ply: u32,
    pub white: String,
    pub black: String,
    pub date: String,
    /// What the user played here in this game.
    pub san: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabNode {
    /// Position the user moves from (also what a book extension analyses).
    pub fen: String,
    /// Earliest 1-based ply the node was reached at.
    pub ply: u32,
    /// Numbered SAN of the earliest path to the node.
    pub line: String,
    pub games: u32,
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    /// The user's repertoire card move here, when one exists.
    pub rep_san: Option<String>,
    /// True when a completed book extension is stored for `fen`.
    pub has_extension: bool,
    /// Σ move damage — the node ranking key.
    pub damage: f64,
    pub moves: Vec<LabMove>,
    pub examples: Vec<LabExample>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeworkRow {
    pub game_id: i64,
    /// Ply of the first significant error (deep-link target).
    pub ply: u32,
    pub white: String,
    pub black: String,
    pub date: String,
    pub swing_cp: i32,
    pub before_cp: i32,
    pub after_cp: i32,
    /// The killer structures this game was tagged with.
    pub structures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabReport {
    pub player: String,
    pub color: String,
    pub ecos: Vec<String>,
    /// Decided/drawn standard-start cohort games walked.
    pub games: u32,
    pub score_pct: f64,
    /// Games with no stored eval pair at any user move — error-checking
    /// is impossible for them and the Lab says so (the one honest
    /// re-analyze button targets exactly these).
    pub unanalyzed_games: u32,
    pub exit: ExitStats,
    pub at_exit: AtExitStats,
    pub errors: ErrorStats,
    /// Middlegame structures by frequency × score deficit, worst first.
    pub structures: Vec<StructureStat>,
    /// Branch nodes ranked by damage.
    pub nodes: Vec<LabNode>,
    /// First-error positions in the killer structures.
    pub homework: Vec<HomeworkRow>,
}

fn pct(points: f64, games: u32) -> f64 {
    if games == 0 {
        0.0
    } else {
        (points / games as f64 * 1000.0).round() / 10.0
    }
}

fn damage_of(points: f64, games: u32) -> f64 {
    let deficit = (0.5 - points / games.max(1) as f64).max(0.0);
    (games as f64 * deficit * 100.0).round() / 100.0
}

/// Median by the lower-middle rank (deterministic for even counts).
fn median(sorted: &[u32]) -> Option<u32> {
    if sorted.is_empty() {
        None
    } else {
        Some(sorted[(sorted.len() - 1) / 2])
    }
}

/// Nearest-rank quantile of an ascending slice.
fn quantile(sorted: &[u32], q: f64) -> Option<u32> {
    if sorted.is_empty() {
        None
    } else {
        let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
        Some(sorted[idx])
    }
}

/// The top killer-structure flags: damage-ranked, damage > 0 only.
fn killer_structures(structures: &[StructureStat]) -> Vec<String> {
    structures
        .iter()
        .filter(|s| s.damage > 0.0)
        .take(KILLER_STRUCTURES)
        .map(|s| s.flag.clone())
        .collect()
}

/// One cohort game with its diagnosis.
struct CohortGame {
    game_id: i64,
    points: f64,
    white: String,
    black: String,
    date: String,
    structures: Vec<String>,
    diag: GameDiag,
}

/// Load and diagnose every cohort game: `player`'s identity-resolved
/// decided/drawn standard-start games of `color`, whose ECO prefix is in
/// `ecos`.
fn load_cohort(
    conn: &Connection,
    player: &str,
    is_white: bool,
    ecos: &[String],
) -> anyhow::Result<Vec<CohortGame>> {
    let ids = crate::identity::resolve_identity_ids(conn, player)?;
    if ids.is_empty() {
        anyhow::bail!("no player named {player:?} in this database");
    }
    if ecos.is_empty() {
        anyhow::bail!("empty ECO set — pick a cohort first");
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let side = if is_white { "white_id" } else { "black_id" };
    let placeholders = ecos
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");

    let theory = crate::fingerprint::theory_set(conn)?;

    let mut stmt = conn.prepare(&format!(
        "SELECT g.id, g.result, g.movetext,
                COALESCE(wp.name, '?'), COALESCE(bp.name, '?'), COALESCE(g.date, '')
         FROM games g
         LEFT JOIN players wp ON wp.id = g.white_id
         LEFT JOIN players bp ON bp.id = g.black_id
         WHERE g.{side} IN ({id_list})
           AND g.start_fen IS NULL
           AND g.result IN (1, 2, 3)
           AND substr(COALESCE(g.eco, ''), 1, 3) IN ({placeholders})
         ORDER BY g.id DESC"
    ))?;
    let rows: Vec<(i64, i64, Vec<u8>, String, String, String)> = stmt
        .query_map(params_from_iter(ecos.iter()), |r| {
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

    let user = if is_white {
        CozyColor::White
    } else {
        CozyColor::Black
    };
    let mut out = Vec::with_capacity(rows.len());
    for (game_id, result, movetext, white, black, date) in rows {
        let points = match (result, is_white) {
            (1, true) | (2, false) => 1.0,
            (2, true) | (1, false) => 0.0,
            (3, _) => 0.5,
            _ => continue,
        };
        let Ok(moves) = decode_game(&Board::default(), &movetext) else {
            continue;
        };
        if moves.is_empty() {
            continue;
        }
        let evals = crate::profile::game_evals(conn, game_id)?;
        let diag = diagnose_game(is_white, &moves, &evals, &theory);

        // Middlegame structure snapshot (the profile's shared tagging).
        let snap = moves.len().min(STRUCTURE_SNAPSHOT_PLY);
        let mut board = Board::default();
        for &mv in &moves[..snap] {
            board.play(mv);
        }
        let structures = crate::profile::structure_flags_at(&board, user);

        out.push(CohortGame {
            game_id,
            points,
            white,
            black,
            date,
            structures,
            diag,
        });
    }
    Ok(out)
}

/// Cohort games that cannot be error-checked (no stored eval pair at any
/// user move) — the target set of the Lab's one re-analyze button.
pub fn cohort_unanalyzed(
    conn: &Connection,
    player: &str,
    is_white: bool,
    ecos: &[String],
) -> anyhow::Result<Vec<i64>> {
    Ok(load_cohort(conn, player, is_white, ecos)?
        .into_iter()
        .filter(|g| !g.diag.analyzed)
        .map(|g| g.game_id)
        .collect())
}

/// Working aggregate for one branch move.
#[derive(Default)]
struct MoveAgg {
    games: u32,
    points: f64,
    eval_sum: i64,
    eval_games: u32,
    in_book: bool,
    replies: HashMap<String, (u32, bool)>,
}

/// Working aggregate for one branch node.
struct NodeAgg {
    fen: String,
    min_ply: u32,
    line: String,
    games: u32,
    moves: HashMap<String, MoveAgg>,
    examples: Vec<LabExample>,
}

/// Build the full Lab report for one cohort. Static database walk — no
/// engine, no writes.
pub fn lab_report(
    conn: &Connection,
    player: &str,
    color: &str,
    ecos: &[String],
) -> anyhow::Result<LabReport> {
    let is_white = match color {
        "white" => true,
        "black" => false,
        other => anyhow::bail!("color must be \"white\" or \"black\", got {other:?}"),
    };
    let games = load_cohort(conn, player, is_white, ecos)?;

    // ---- verdict aggregates ----
    let mut points = 0.0;
    let mut unanalyzed = 0u32;
    let mut exit_plies: Vec<u32> = Vec::new();
    let mut still_in_book = 0u32;
    let mut at_exit = AtExitStats {
        evaluated: 0,
        equal: 0,
        better: 0,
        worse: 0,
    };
    let mut analyzed_games = 0u32;
    let mut error_plies: Vec<u32> = Vec::new();
    let mut middlegame_error_plies: Vec<u32> = Vec::new();
    let mut book_phase_errors = 0u32;
    let mut structure_agg: HashMap<String, (u32, f64)> = HashMap::new();
    let mut nodes: HashMap<u64, NodeAgg> = HashMap::new();

    for g in &games {
        points += g.points;
        if g.diag.analyzed {
            analyzed_games += 1;
        } else {
            unanalyzed += 1;
        }
        match g.diag.exit_ply {
            Some(e) => exit_plies.push(e),
            None => still_in_book += 1,
        }
        if let Some(cp) = g.diag.eval_at_exit_cp {
            at_exit.evaluated += 1;
            if cp > EQUALISH_CP {
                at_exit.better += 1;
            } else if cp < -EQUALISH_CP {
                at_exit.worse += 1;
            } else {
                at_exit.equal += 1;
            }
        }
        if let Some(err) = &g.diag.first_error {
            error_plies.push(err.ply);
            if err.in_book_phase {
                book_phase_errors += 1;
            } else {
                middlegame_error_plies.push(err.ply);
            }
        }
        for flag in &g.structures {
            let e = structure_agg.entry(flag.clone()).or_default();
            e.0 += 1;
            e.1 += g.points;
        }
        for obs in &g.diag.observations {
            let node = nodes.entry(obs.node_hash).or_insert_with(|| NodeAgg {
                fen: obs.node_fen.clone(),
                min_ply: obs.node_ply,
                line: obs.line.clone(),
                games: 0,
                moves: HashMap::new(),
                examples: Vec::new(),
            });
            node.games += 1;
            if obs.node_ply < node.min_ply {
                node.min_ply = obs.node_ply;
                node.line = obs.line.clone();
            }
            if node.examples.len() < MAX_NODE_EXAMPLES {
                node.examples.push(LabExample {
                    game_id: g.game_id,
                    ply: obs.node_ply,
                    white: g.white.clone(),
                    black: g.black.clone(),
                    date: g.date.clone(),
                    san: obs.san.clone(),
                });
            }
            let m = node.moves.entry(obs.san.clone()).or_default();
            m.games += 1;
            m.points += g.points;
            m.in_book = obs.after_in_book;
            if let Some(cp) = obs.eval_after_cp {
                m.eval_sum += cp as i64;
                m.eval_games += 1;
            }
            if let Some((reply_san, reply_in_book)) = &obs.reply {
                let r = m
                    .replies
                    .entry(reply_san.clone())
                    .or_insert((0, *reply_in_book));
                r.0 += 1;
            }
        }
    }

    exit_plies.sort_unstable();
    error_plies.sort_unstable();
    middlegame_error_plies.sort_unstable();

    let mut structures: Vec<StructureStat> = structure_agg
        .into_iter()
        .map(|(flag, (n, pts))| StructureStat {
            flag,
            games: n,
            score_pct: pct(pts, n),
            damage: damage_of(pts, n),
        })
        .collect();
    structures.sort_by(|a, b| {
        b.damage
            .total_cmp(&a.damage)
            .then(b.games.cmp(&a.games))
            .then(a.flag.cmp(&b.flag))
    });

    // ---- branch nodes ----
    let mut card_lookup = conn.prepare_cached(
        "SELECT c.expected_san FROM repertoire_cards c
         JOIN repertoires r ON r.id = c.repertoire_id
         WHERE r.color = ?1 AND c.position_hash = ?2
         ORDER BY c.id LIMIT 1",
    )?;
    let mut node_rows: Vec<(u64, LabNode)> = Vec::new();
    for (hash, agg) in nodes {
        let mut moves: Vec<LabMove> = agg
            .moves
            .into_iter()
            .map(|(san, m)| {
                let mut replies: Vec<LabReply> = m
                    .replies
                    .into_iter()
                    .map(|(san, (n, in_book))| LabReply {
                        san,
                        games: n,
                        in_book,
                    })
                    .collect();
                replies.sort_by(|a, b| b.games.cmp(&a.games).then(a.san.cmp(&b.san)));
                LabMove {
                    san,
                    games: m.games,
                    score_pct: pct(m.points, m.games),
                    avg_eval_cp: (m.eval_games > 0)
                        .then(|| (m.eval_sum as f64 / m.eval_games as f64).round() as i32),
                    eval_games: m.eval_games,
                    in_book: m.in_book,
                    in_rep: false, // filled below once the card is known
                    damage: damage_of(m.points, m.games),
                    replies,
                }
            })
            .collect();
        moves.sort_by(|a, b| {
            b.damage
                .total_cmp(&a.damage)
                .then(b.games.cmp(&a.games))
                .then(a.san.cmp(&b.san))
        });
        let damage = (moves.iter().map(|m| m.damage).sum::<f64>() * 100.0).round() / 100.0;
        node_rows.push((
            hash,
            LabNode {
                fen: agg.fen,
                ply: agg.min_ply,
                line: agg.line,
                games: agg.games,
                eco: None,
                opening_name: None,
                rep_san: None,
                has_extension: false,
                damage,
                moves,
                examples: agg.examples,
            },
        ));
    }
    node_rows.sort_by(|(_, a), (_, b)| {
        b.damage
            .total_cmp(&a.damage)
            .then(b.games.cmp(&a.games))
            .then(a.ply.cmp(&b.ply))
            .then(a.fen.cmp(&b.fen))
    });
    node_rows.truncate(MAX_NODES);
    // Decorations only for the surviving nodes (lookups are per-node).
    let mut node_list = Vec::with_capacity(node_rows.len());
    for (hash, mut node) in node_rows {
        if let Some((e, n)) = crate::eco::classify_hash(conn, hash)? {
            node.eco = Some(e);
            node.opening_name = Some(n);
        }
        node.rep_san = card_lookup
            .query_row(params![color, hash as i64], |r| r.get::<_, String>(0))
            .optional()?;
        if let Some(rep) = &node.rep_san {
            for m in &mut node.moves {
                m.in_rep = &m.san == rep;
            }
        }
        node.has_extension = crate::triage::latest_book_extension(conn, &node.fen)?.is_some();
        node_list.push(node);
    }

    // ---- homework: first errors in the killer structures ----
    let killers = killer_structures(&structures);
    let mut homework: Vec<HomeworkRow> = games
        .iter()
        .filter_map(|g| {
            let err = g.diag.first_error.as_ref()?;
            let tagged: Vec<String> = g
                .structures
                .iter()
                .filter(|f| killers.contains(f))
                .cloned()
                .collect();
            if tagged.is_empty() {
                return None;
            }
            Some(HomeworkRow {
                game_id: g.game_id,
                ply: err.ply,
                white: g.white.clone(),
                black: g.black.clone(),
                date: g.date.clone(),
                swing_cp: err.swing_cp,
                before_cp: err.before_cp,
                after_cp: err.after_cp,
                structures: tagged,
            })
        })
        .collect();
    homework.sort_by(|a, b| b.swing_cp.cmp(&a.swing_cp).then(b.game_id.cmp(&a.game_id)));
    homework.truncate(MAX_HOMEWORK);

    let games_count = games.len() as u32;
    Ok(LabReport {
        player: player.to_string(),
        color: color.to_string(),
        ecos: ecos.to_vec(),
        games: games_count,
        score_pct: pct(points, games_count),
        unanalyzed_games: unanalyzed,
        exit: ExitStats {
            left_book: exit_plies.len() as u32,
            still_in_book,
            median_exit_ply: median(&exit_plies),
        },
        at_exit,
        errors: ErrorStats {
            analyzed_games,
            games_with_error: error_plies.len() as u32,
            book_phase: book_phase_errors,
            middlegame: middlegame_error_plies.len() as u32,
            no_error_found: analyzed_games - error_plies.len() as u32,
            median_error_ply: median(&error_plies),
            middlegame_p25_ply: quantile(&middlegame_error_plies, 0.25),
            middlegame_p75_ply: quantile(&middlegame_error_plies, 0.75),
        },
        structures,
        nodes: node_list,
        homework,
    })
}

// ---------------------------------------------------------------------------
// candidate fit: where does a line lead, structurally?
// ---------------------------------------------------------------------------

/// Structure flags of the position a candidate line leads to, from the
/// point of view of the side to move at `fen` (the user at a branch
/// node). Plays as many SANs as replay cleanly; the caller joins the
/// flags against the cached profile's structure scores.
pub fn candidate_structures(fen: &str, sans: &[String]) -> anyhow::Result<Vec<String>> {
    let mut board: Board = fen
        .parse()
        .map_err(|e| anyhow::anyhow!("bad FEN {fen:?}: {e:?}"))?;
    let user = board.side_to_move();
    for san in sans {
        let Ok(mv) = crate::san::parse_san(&board, san) else {
            break;
        };
        board.play(mv);
    }
    Ok(crate::profile::structure_flags_at(&board, user))
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

    /// Theory set from explicit "book lines": every position after every
    /// prefix of every line — exactly how ensure_openings builds the real
    /// one, but fully under test control.
    fn theory_of(lines: &[&[&str]]) -> HashSet<u64> {
        let mut set = HashSet::new();
        for line in lines {
            let mut board = Board::default();
            for san in line.iter() {
                let mv = crate::san::parse_san(&board, san).unwrap();
                board.play(mv);
                set.insert(position_hash(&board));
            }
        }
        set
    }

    const RUY: &[&str] = &[
        "e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O", "Be7",
    ];

    #[test]
    fn diagnose_detects_exit_and_collects_branch_observations() {
        let theory = theory_of(&[RUY, &["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5"]]);
        let evals = HashMap::new();

        // Exit by the opponent: ...Rg8 is not in the book.
        let (_, moves) = play_sans(&[
            "e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O", "Rg8", "d4", "h6",
        ]);
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert_eq!(d.exit_ply, Some(10), "10th ply ...Rg8 leaves theory");
        assert!(!d.analyzed, "no evals → honestly unanalyzed");
        assert_eq!(d.first_error, None);
        assert_eq!(d.eval_at_exit_cp, None);

        // Branch observations: White moved from 5 in-book nodes; the walk
        // stops at the exit, so the post-exit 6.d4 contributes nothing.
        assert_eq!(d.observations.len(), 5);
        let first = &d.observations[0];
        assert_eq!((first.node_ply, first.san.as_str()), (1, "e4"));
        assert_eq!(first.node_fen, Board::default().to_string());
        assert_eq!(first.line, "");
        assert!(first.after_in_book, "1.e4 is in the test theory");
        assert_eq!(first.reply.as_deref_pair(), Some(("e5", true)));
        let third = &d.observations[2];
        assert_eq!((third.node_ply, third.san.as_str()), (5, "Bb5"));
        assert_eq!(third.line, "1. e4 e5 2. Nf3 Nc6");
        let fifth = &d.observations[4];
        assert_eq!((fifth.node_ply, fifth.san.as_str()), (9, "O-O"));
        assert_eq!(
            fifth.reply.as_deref_pair(),
            Some(("Rg8", false)),
            "the reply that left book is recorded as out-of-book"
        );

        // Exit by the user: 3.Bd3 is not in the book — the observation at
        // the node still records the off-book move (that IS the branch
        // where the user goes wrong), and the exit lands on its ply.
        let (_, moves) = play_sans(&["e4", "e5", "Nf3", "Nc6", "Bd3", "Bc5"]);
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert_eq!(d.exit_ply, Some(5));
        let last = d.observations.last().unwrap();
        assert_eq!((last.san.as_str(), last.after_in_book), ("Bd3", false));
        assert_eq!(last.reply, None, "off-book moves collect no reply");

        // Still in book through the whole (short) game: no exit.
        let (_, moves) = play_sans(RUY);
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert_eq!(d.exit_ply, None);
        assert_eq!(d.observations.len(), 5);
    }

    /// Helper for the reply assertions above.
    trait AsDerefPair {
        fn as_deref_pair(&self) -> Option<(&str, bool)>;
    }
    impl AsDerefPair for Option<(String, bool)> {
        fn as_deref_pair(&self) -> Option<(&str, bool)> {
            self.as_ref().map(|(s, b)| (s.as_str(), *b))
        }
    }

    #[test]
    fn first_error_uses_user_pov_threshold_for_both_colors() {
        let theory = theory_of(&[RUY]);
        let (_, moves) = play_sans(&[
            "e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O", "Rg8", "d4", "h6",
        ]);

        // White POV evals (the game_evals convention): fine until White's
        // 11th ply drops +30 → −150 (swing 180 ≥ 120).
        let evals: HashMap<u16, i32> = [(8, 20), (9, 25), (10, 30), (11, -150), (12, -140)].into();
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert!(d.analyzed);
        let err = d.first_error.expect("error found");
        assert_eq!((err.ply, err.swing_cp), (11, 180));
        assert_eq!((err.before_cp, err.after_cp), (30, -150));
        assert!(!err.in_book_phase, "ply 11 is after the exit at 10");
        assert_eq!(d.eval_at_exit_cp, Some(30), "eval at the exit ply");

        // Same numbers, user = Black: the White-POV drop 30 → −150 is a
        // GAIN for Black; Black's own moves (even plies) never drop ≥ 120
        // here — no error, and every POV value flips sign.
        let d = diagnose_game(false, &moves, &evals, &theory);
        assert!(d.analyzed);
        assert_eq!(d.first_error, None);
        assert_eq!(d.eval_at_exit_cp, Some(-30));

        // Black user with a real Black error: White-POV −40 → +130 after
        // Black's 10th ply is a 170 cp drop from Black's POV.
        let evals: HashMap<u16, i32> = [(9, -40), (10, 130)].into();
        let d = diagnose_game(false, &moves, &evals, &theory);
        let err = d.first_error.expect("black error found");
        assert_eq!((err.ply, err.swing_cp), (10, 170));
        assert_eq!((err.before_cp, err.after_cp), (40, -130));
        assert!(err.in_book_phase, "ply 10 IS the exit ply — book phase");

        // A drop below the threshold is not an error.
        let evals: HashMap<u16, i32> = [(10, 30), (11, -80)].into();
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert!(d.analyzed);
        assert_eq!(d.first_error, None, "110 cp < 120 cp threshold");

        // Eval-at-exit fallback: nothing at ply 10, the ply-9 value is
        // used instead.
        let evals: HashMap<u16, i32> = [(9, 45)].into();
        let d = diagnose_game(true, &moves, &evals, &theory);
        assert_eq!(d.eval_at_exit_cp, Some(45));
        assert!(!d.analyzed, "a lone eval forms no (before, after) pair");
    }

    #[test]
    fn median_quantile_and_killer_helpers() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[7]), Some(7));
        assert_eq!(median(&[6, 10, 10]), Some(10));
        assert_eq!(median(&[9, 15]), Some(9), "lower-middle rank");
        assert_eq!(quantile(&[], 0.25), None);
        assert_eq!(quantile(&[18, 20, 22, 26], 0.25), Some(20));
        assert_eq!(quantile(&[18, 20, 22, 26], 0.75), Some(22));
        assert_eq!(quantile(&[15], 0.75), Some(15));

        let stats = vec![
            StructureStat {
                flag: "own-isolated-pawn".into(),
                games: 4,
                score_pct: 25.0,
                damage: 1.0,
            },
            StructureStat {
                flag: "own-passed-pawn".into(),
                games: 3,
                score_pct: 66.7,
                damage: 0.0,
            },
        ];
        assert_eq!(killer_structures(&stats), vec!["own-isolated-pawn"]);
        assert!(killer_structures(&[]).is_empty());
    }

    /// Fixture cohort, user "Lab, Tester" as White (one game under the
    /// lexical variant "Tester Lab" to exercise identity resolution):
    /// - G1 (win):  Ruy through 5.O-O, Black leaves book with 5...Rg8.
    /// - G2 (loss): same line and exit.
    /// - G3 (loss): Italian; Black leaves book with 3...Na5.
    ///
    /// A Black game and a foreign-player game must not contaminate.
    const GAMES: &str = r#"[Event "Club"]
[White "Lab, Tester"]
[Black "Erste, Anna"]
[Date "2026.01.10"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Rg8 6. d4 h6 7. dxe5 Nxe4
8. Qd5 Nc5 9. Nc3 d6 10. exd6 Bxd6 1-0

[Event "Club"]
[White "Tester Lab"]
[Black "Zweite, Bea"]
[Date "2026.02.11"]
[Result "0-1"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Rg8 6. d4 h6 7. d5 Nb8
8. c4 Bb4 9. Nc3 Bxc3 10. bxc3 0-1

[Event "Club"]
[White "Lab, Tester"]
[Black "Dritte, Cora"]
[Date "2026.03.12"]
[Result "0-1"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Na5 4. Bd5 c6 5. Bb3 Nxb3 6. axb3 d5 0-1

[Event "Club"]
[White "Vierte, Dana"]
[Black "Lab, Tester"]
[Date "2026.04.13"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 1-0

[Event "Club"]
[White "Fremd, Emil"]
[Black "Fremd, Fritz"]
[Date "2026.05.14"]
[Result "1/2-1/2"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1/2-1/2
"#;

    fn plant(conn: &Connection, game_id: i64, ply: i64, kind: &str, cp: i64) {
        conn.execute(
            "INSERT INTO analyses (game_id, ply, kind, engine, eval_cp)
             VALUES (?1, ?2, ?3, 'Test Engine', ?4)",
            params![game_id, ply, kind, cp],
        )
        .unwrap();
    }

    fn fixture_db() -> (tempfile::TempDir, Connection) {
        let (dir, conn) = open_db();
        let st = import_pgn(&conn, &source(), Cursor::new(GAMES)).unwrap();
        assert_eq!(st.games_imported, 5, "failures: {:?}", st.failures);

        // G1: equal-ish at the exit (White-POV +20 at ply 10), first
        // error at White's ply 11 (+20 → −150, swing 170, middlegame).
        // Fresh rows are side-to-move POV: even ply = White to move =
        // White POV as-is; odd ply = negate.
        plant(&conn, 1, 10, "fresh", 20); // white POV +20
        plant(&conn, 1, 11, "fresh", 150); // black to move +150 = White −150

        // G2: already worse at the exit (−120), book-phase error at
        // White's ply 9 (+10 → −160, swing 170 ≤ exit ply 10). Legacy
        // rows are White-POV directly.
        plant(&conn, 2, 8, "legacy-import", 10);
        plant(&conn, 2, 9, "legacy-import", -160);
        plant(&conn, 2, 10, "legacy-import", -120);

        // G3: no evals at all — unanalyzed.
        (dir, conn)
    }

    fn cohort_ecos(conn: &Connection) -> Vec<String> {
        // The cohort key is whatever the importer actually tagged the
        // fixture games with (Ruy + Italian codes); the report filters by
        // color, so passing every fixture code forms the White cohort.
        let mut stmt = conn
            .prepare("SELECT DISTINCT substr(eco,1,3) FROM games WHERE eco IS NOT NULL")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn report_aggregates_verdict_numbers_and_ranks_branches() {
        let (_dir, conn) = fixture_db();
        let ecos = cohort_ecos(&conn);
        let r = lab_report(&conn, "Lab, Tester", "white", &ecos).unwrap();

        // Cohort scope: 3 White games (the Black game and the foreign
        // game are out), identity-resolved across both name forms.
        assert_eq!(r.games, 3);
        assert_eq!(r.score_pct, 33.3, "1 win / 2 losses");
        assert_eq!(r.unanalyzed_games, 1, "G3 has no evals");

        // Exits: G1/G2 at ply 10 (...Rg8), G3 at ply 6 (...Na5).
        assert_eq!((r.exit.left_book, r.exit.still_in_book), (3, 0));
        assert_eq!(r.exit.median_exit_ply, Some(10));

        // At exit: G1 +20 = equal, G2 −120 = worse, G3 unevaluated.
        assert_eq!(
            (
                r.at_exit.evaluated,
                r.at_exit.equal,
                r.at_exit.better,
                r.at_exit.worse
            ),
            (2, 1, 0, 1)
        );

        // Errors: G1 middlegame (ply 11 > exit 10), G2 book phase
        // (ply 9 ≤ exit 10).
        assert_eq!(r.errors.analyzed_games, 2);
        assert_eq!(r.errors.games_with_error, 2);
        assert_eq!((r.errors.book_phase, r.errors.middlegame), (1, 1));
        assert_eq!(r.errors.no_error_found, 0);
        assert_eq!(r.errors.median_error_ply, Some(9), "[9, 11] lower-middle");
        assert_eq!(r.errors.middlegame_p25_ply, Some(11));
        assert_eq!(r.errors.middlegame_p75_ply, Some(11));

        // Structures come from the shared profile tagging at the snapshot
        // ply — recompute the expectation directly for one game.
        let (b3, _) = play_sans(&[
            "e4", "e5", "Nf3", "Nc6", "Bc4", "Na5", "Bd5", "c6", "Bb3", "Nxb3", "axb3", "d5",
        ]);
        let expect3 = crate::profile::structure_flags_at(&b3, CozyColor::White);
        assert!(
            expect3.contains(&"own-doubled-pawns".to_string()),
            "axb3 doubles the b-pawns: {expect3:?}"
        );
        for flag in &expect3 {
            let row = r
                .structures
                .iter()
                .find(|s| &s.flag == flag)
                .unwrap_or_else(|| panic!("missing structure {flag}"));
            assert!(row.games >= 1);
        }

        // Branch node after 1.e4 e5 2.Nf3 Nc6: Bb5 twice (1 win 1 loss →
        // damage 0), Bc4 once (loss → damage 0.5). The node's damage is
        // the sum; Bc4 ranks first inside the node.
        let (nc6, _) = play_sans(&["e4", "e5", "Nf3", "Nc6"]);
        let node = r
            .nodes
            .iter()
            .find(|n| n.fen == nc6.to_string())
            .expect("2...Nc6 node present");
        assert_eq!(node.games, 3);
        assert_eq!(node.ply, 5);
        assert_eq!(node.line, "1. e4 e5 2. Nf3 Nc6");
        assert_eq!(node.damage, 0.5);
        assert_eq!(node.moves.len(), 2);
        assert_eq!(node.moves[0].san, "Bc4");
        assert_eq!((node.moves[0].games, node.moves[0].damage), (1, 0.5));
        assert_eq!(node.moves[0].score_pct, 0.0);
        assert!(node.moves[0].in_book, "3.Bc4 is book");
        assert_eq!(node.moves[1].san, "Bb5");
        assert_eq!((node.moves[1].games, node.moves[1].damage), (2, 0.0));
        assert_eq!(node.moves[1].score_pct, 50.0);
        assert!(
            node.eco.is_some() && node.opening_name.is_some(),
            "book node is named"
        );
        assert_eq!(node.rep_san, None, "no repertoire yet");
        assert!(!node.has_extension);
        // Games walk newest-first (id DESC): G3's Bc4 is the first example.
        assert_eq!(node.examples.len(), 3);
        assert_eq!(
            (node.examples[0].game_id, node.examples[0].san.as_str()),
            (3, "Bc4")
        );
        assert_eq!(node.examples[0].ply, 5);

        // Coverage raw material: after 3.Bb5 both games saw ...a6 (still
        // book); after 3.Bc4 the one reply ...Na5 left book.
        let bb5 = &node.moves[1];
        assert_eq!(bb5.replies.len(), 1);
        assert_eq!(
            (
                bb5.replies[0].san.as_str(),
                bb5.replies[0].games,
                bb5.replies[0].in_book
            ),
            ("a6", 2, true)
        );
        let bc4 = &node.moves[0];
        assert_eq!(bc4.replies.len(), 1);
        assert_eq!(
            (bc4.replies[0].san.as_str(), bc4.replies[0].in_book),
            ("Na5", false)
        );

        // Branch-eval column: the ply-9 node (after 4...Nf6) carries
        // O-O's stored after-evals: G1 fresh at ply 9 is absent, but G2
        // legacy −160 at ply 9 → avg −160 over 1 evaluated game.
        let (nf6, _) = play_sans(&["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6"]);
        let node9 = r
            .nodes
            .iter()
            .find(|n| n.fen == nf6.to_string())
            .expect("ply-9 node present");
        let oo = node9.moves.iter().find(|m| m.san == "O-O").unwrap();
        assert_eq!((oo.avg_eval_cp, oo.eval_games), (Some(-160), 1));

        // Node ranking: damage-bearing nodes come first.
        assert!(r.nodes[0].damage >= r.nodes.last().unwrap().damage);

        // Homework: both losses end with doubled white pawns (10.bxc3 in
        // G2, 6.axb3 in G3), so own-doubled-pawns is a killer structure
        // (2 games, 0 points → damage 1.0). G2's book-phase error at ply
        // 9 is listed; G3 (unanalyzed) has no first error and cannot
        // appear; G1's error only appears if G1 shares a killer flag.
        let doubled = r
            .structures
            .iter()
            .find(|s| s.flag == "own-doubled-pawns")
            .expect("doubled-pawn structure aggregated");
        assert_eq!(
            (doubled.games, doubled.score_pct, doubled.damage),
            (2, 0.0, 1.0)
        );
        let killers = killer_structures(&r.structures);
        assert!(killers.contains(&"own-doubled-pawns".to_string()));
        let hw2 = r
            .homework
            .iter()
            .find(|h| h.game_id == 2)
            .expect("G2's first error is homework");
        assert_eq!((hw2.ply, hw2.swing_cp), (9, 170));
        assert_eq!((hw2.before_cp, hw2.after_cp), (10, -160));
        assert!(hw2.structures.contains(&"own-doubled-pawns".to_string()));
        for h in &r.homework {
            assert!(h.structures.iter().all(|f| killers.contains(f)));
            assert!(h.swing_cp >= FIRST_ERROR_CP);
            assert_ne!(h.game_id, 3, "unanalyzed games have no first error");
        }

        // Wire shape: camelCase.
        let json = serde_json::to_string(&r).unwrap();
        for needle in [
            "\"unanalyzedGames\":",
            "\"medianExitPly\":",
            "\"stillInBook\":",
            "\"analyzedGames\":",
            "\"bookPhase\":",
            "\"middlegameP25Ply\":",
            "\"scorePct\":",
            "\"avgEvalCp\":",
            "\"inBook\":",
            "\"inRep\":",
            "\"repSan\":",
            "\"hasExtension\":",
            "\"swingCp\":",
            "\"gameId\":",
        ] {
            assert!(json.contains(needle), "missing {needle}");
        }

        // Unknown player fails cleanly; the whole Lab is engine-free.
        assert!(lab_report(&conn, "Nobody, At All", "white", &ecos).is_err());
        assert!(lab_report(&conn, "Lab, Tester", "purple", &ecos).is_err());
        assert!(lab_report(&conn, "Lab, Tester", "white", &[]).is_err());
        assert_eq!(crate::engine::spawn_count(), 0);
    }

    #[test]
    fn repertoire_and_extension_markers_flip_and_unanalyzed_is_listed() {
        let (_dir, conn) = fixture_db();
        let ecos = cohort_ecos(&conn);

        // Unanalyzed cohort games — the honest re-analyze target set.
        let un = cohort_unanalyzed(&conn, "Lab, Tester", true, &ecos).unwrap();
        assert_eq!(un, vec![3], "only G3 lacks eval pairs");

        // Adopt a repertoire line covering 3.Bb5: the node's repSan and
        // Bb5's inRep flip; Bc4 stays unmarked.
        let rep = crate::repertoire::ensure_repertoire(
            &conn,
            kibitz_profile::Color::White,
            "main",
            &source(),
        )
        .unwrap();
        let now = crate::repertoire::now_utc(&conn).unwrap();
        let sans: Vec<String> = ["e4", "e5", "Nf3", "Nc6", "Bb5"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        crate::repertoire::add_line(
            &conn,
            rep,
            kibitz_profile::Color::White,
            &Board::default(),
            &sans,
            &now,
        )
        .unwrap();

        // Store a book extension for the 2...Nc6 node position.
        let (nc6, _) = play_sans(&["e4", "e5", "Nf3", "Nc6"]);
        crate::triage::store_book_extension(
            &conn,
            &nc6.to_string(),
            "Test Engine",
            30,
            4,
            &[crate::triage::CandidateLine {
                sans: vec!["Bb5".into(), "a6".into(), "Ba4".into()],
                score_cp: 30,
                mate: None,
            }],
        )
        .unwrap();

        let r = lab_report(&conn, "Lab, Tester", "white", &ecos).unwrap();
        let node = r
            .nodes
            .iter()
            .find(|n| n.fen == nc6.to_string())
            .expect("2...Nc6 node");
        assert_eq!(node.rep_san.as_deref(), Some("Bb5"));
        assert!(node.has_extension);
        let bb5 = node.moves.iter().find(|m| m.san == "Bb5").unwrap();
        let bc4 = node.moves.iter().find(|m| m.san == "Bc4").unwrap();
        assert!(bb5.in_rep && !bc4.in_rep);
        assert_eq!(crate::engine::spawn_count(), 0);
    }

    #[test]
    fn cohorts_group_by_family_and_merge_nimzo_codes() {
        let (_dir, conn) = open_db();
        let pgn = r#"[Event "Club"]
[White "Lab, Tester"]
[Black "Nimzo, One"]
[Result "1-0"]

1. d4 Nf6 2. c4 e6 3. Nc3 Bb4 4. Qc2 O-O 5. a3 Bxc3+ 6. Qxc3 b6 1-0

[Event "Club"]
[White "Nimzo, Two"]
[Black "Lab, Tester"]
[Result "0-1"]

1. d4 Nf6 2. c4 e6 3. Nc3 Bb4 4. e3 O-O 5. Bd3 d5 6. Nf3 c5 0-1

[Event "Club"]
[White "Nimzo, Three"]
[Black "Lab, Tester"]
[Result "1/2-1/2"]

1. d4 Nf6 2. c4 e6 3. Nc3 Bb4 4. f3 d5 5. a3 Bxc3+ 6. bxc3 c5 1/2-1/2

[Event "Club"]
[White "Lab, Tester"]
[Black "Sizilianer, Vier"]
[Result "0-1"]

1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6 0-1
"#;
        let st = import_pgn(&conn, &source(), Cursor::new(pgn)).unwrap();
        assert_eq!(st.games_imported, 4, "failures: {:?}", st.failures);

        let rows = cohorts(&conn, "Lab, Tester").unwrap();
        // The two Black Nimzo games (E2x/E4x variations) merge into ONE
        // family cohort; the White games group separately by color.
        let nimzo = rows
            .iter()
            .find(|r| r.color == "black" && r.family.contains("Nimzo"))
            .unwrap_or_else(|| panic!("Nimzo family cohort missing: {rows:?}"));
        assert_eq!(nimzo.games, 2);
        assert!(
            nimzo.ecos.len() >= 2,
            "distinct codes merged: {:?}",
            nimzo.ecos
        );
        assert!(nimzo.eco_min < nimzo.eco_max);
        assert!(nimzo.ecos.iter().all(|e| e.starts_with('E')));

        let sicilian = rows
            .iter()
            .find(|r| r.color == "white" && r.family.contains("Sicilian"))
            .expect("Sicilian cohort");
        assert_eq!(sicilian.games, 1);

        // Cohort → report round trip: the cohort's own ECO set drives it.
        let r = lab_report(&conn, "Lab, Tester", "black", &nimzo.ecos).unwrap();
        assert_eq!(r.games, 2);
        assert_eq!(r.score_pct, 75.0, "one win, one draw as Black");

        // Wire shape.
        let json = serde_json::to_string(&rows).unwrap();
        for needle in ["\"ecoMin\":", "\"ecoMax\":", "\"family\":", "\"ecos\":"] {
            assert!(json.contains(needle), "missing {needle}");
        }
        assert!(cohorts(&conn, "Nobody, At All").is_err());
        assert_eq!(crate::engine::spawn_count(), 0);
    }

    #[test]
    fn candidate_structures_replays_the_line_for_the_node_side() {
        // From the 2...Nc6 node (White to move), a line into an exchange
        // on b3 doubles White's pawns — flags must match the shared
        // classification computed directly.
        let (nc6, _) = play_sans(&["e4", "e5", "Nf3", "Nc6"]);
        let sans: Vec<String> = ["Bc4", "Na5", "Bd5", "c6", "Bb3", "Nxb3", "axb3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let flags = candidate_structures(&nc6.to_string(), &sans).unwrap();
        let (end, _) = play_sans(&[
            "e4", "e5", "Nf3", "Nc6", "Bc4", "Na5", "Bd5", "c6", "Bb3", "Nxb3", "axb3",
        ]);
        assert_eq!(
            flags,
            crate::profile::structure_flags_at(&end, CozyColor::White)
        );
        assert!(flags.contains(&"own-doubled-pawns".to_string()));

        // An unparsable tail truncates instead of failing.
        let bad: Vec<String> = ["Bc4", "Qq9"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            candidate_structures(&nc6.to_string(), &bad).unwrap(),
            candidate_structures(&nc6.to_string(), &bad[..1]).unwrap()
        );
        assert!(candidate_structures("not a fen", &[]).is_err());
    }

    /// The per-game walk must stay fast enough for a multi-thousand-game
    /// cohort computed on demand (no cache, no migration). This measures
    /// the pure decode+diagnose path over 2 000 iterations of a 24-ply
    /// game against a realistic-size theory set and prints the per-game
    /// figure for the run report.
    #[test]
    fn walk_performance_on_a_synthetic_cohort_scale() {
        let (_dir, conn) = open_db();
        let theory = crate::fingerprint::theory_set(&conn).unwrap();
        assert!(theory.len() > 5_000, "real dataset loaded");
        let (_, moves) = play_sans(&[
            "e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O", "Rg8", "d4", "h6", "dxe5",
            "Nxe4", "Qd5", "Nc5", "Nc3", "d6", "exd6", "Bxd6", "Bxc6", "bxc6", "Qxd6", "cxd6",
        ]);
        let blob = crate::movebin::encode_game(&Board::default(), &moves).unwrap();
        let evals: HashMap<u16, i32> = (1..=24).map(|p| (p as u16, p)).collect();

        let n = 2_000u32;
        let start = std::time::Instant::now();
        let mut exits = 0u32;
        for _ in 0..n {
            let decoded = decode_game(&Board::default(), &blob).unwrap();
            let d = diagnose_game(true, &decoded, &evals, &theory);
            if d.exit_ply.is_some() {
                exits += 1;
            }
        }
        let elapsed = start.elapsed();
        assert_eq!(exits, n);
        let per_game_us = elapsed.as_micros() as f64 / n as f64;
        println!("opening_lab walk: {n} games in {elapsed:?} ({per_game_us:.1} µs/game)");
        // Generous bound (heavily loaded machines): 5k games must stay
        // interactive. 30 s / 2 000 iterations would still mean 15 ms per
        // game — far above anything observed; this guards regressions
        // only.
        assert!(
            elapsed.as_secs() < 30,
            "walk too slow: {per_game_us:.1} µs/game"
        );
    }
}
