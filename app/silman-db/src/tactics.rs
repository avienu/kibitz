//! Tactics trainer (ROADMAP Phase 5): Lichess CC0 puzzle import, drill
//! selection (rated / motif-filtered / weakness-weighted / Woodpecker /
//! speed), solve verification, attempt history and the user tactics rating.
//!
//! Engine-off principle (CLAUDE.md #6): NOTHING in this module spawns an
//! engine. Solve checking is exact-match against the stored solution line,
//! except that any played move which delivers checkmate is accepted —
//! cozy-chess verifies the mate statically.
//!
//! Rating system (decided here, kept deliberately simple): plain Elo with a
//! provisional K schedule, puzzle ratings fixed.
//!   expected = 1 / (1 + 10^((puzzle - user) / 400))
//!   new      = user + K * (score - expected),   score = 1 solved / 0 failed
//!   K        = 40 for the user's first 30 rated attempts, then 20
//!   clamped to [500, 3200]
//! Only `rated`, `motif` and `weakness` attempts move the rating; Woodpecker
//! cycles and the Heisman speed drill are repetition training and record
//! attempt history only.

use std::collections::HashMap;
use std::io::BufRead;
use std::time::{Duration, Instant};

use cozy_chess::{Board, GameStatus, Move, Piece, Square};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::import::SourceInfo;

// ---------------------------------------------------------------------------
// Deterministic RNG (SplitMix64): selection stays reproducible under a seed,
// which the distribution tests rely on. No rand dependency.
// ---------------------------------------------------------------------------

/// SplitMix64 PRNG; deterministic for a given seed on every platform.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        self.next() % n
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------------------
// Import (streaming, batched transactions)
// ---------------------------------------------------------------------------

/// Rows per transaction. 5M rows import as ~1000 transactions; memory use
/// stays flat because lines are processed one at a time.
const IMPORT_BATCH: u64 = 5_000;

#[derive(Debug, Clone, Copy, Default)]
pub struct PuzzleImportOptions {
    /// Skip puzzles whose Popularity (Lichess: -100..100) is below this.
    pub min_popularity: Option<i64>,
    /// Stop after importing this many puzzles (post-filter).
    pub max_rows: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleImportStats {
    pub imported: u64,
    pub duplicates_skipped: u64,
    pub filtered_out: u64,
    pub malformed: u64,
    #[serde(skip)]
    pub elapsed: Duration,
}

/// One parsed CSV row. Lichess columns: PuzzleId,FEN,Moves,Rating,
/// RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags. None of
/// the columns we consume can contain a comma, so a plain split suffices.
struct CsvRow<'a> {
    id: &'a str,
    fen: &'a str,
    moves: &'a str,
    rating: i64,
    rating_deviation: i64,
    popularity: i64,
    nb_plays: i64,
    themes: &'a str,
}

fn parse_csv_row(line: &str) -> Option<CsvRow<'_>> {
    let f: Vec<&str> = line.split(',').collect();
    if f.len() < 8 || f[0].is_empty() || f[1].is_empty() || f[2].is_empty() {
        return None;
    }
    Some(CsvRow {
        id: f[0],
        fen: f[1],
        moves: f[2],
        rating: f[3].parse().ok()?,
        rating_deviation: f[4].parse().ok()?,
        popularity: f[5].parse().ok()?,
        nb_plays: f[6].parse().ok()?,
        themes: f[7],
    })
}

/// Stream the Lichess puzzle CSV into the `puzzles` table with a provenance
/// row in `sources`, in batched transactions. Re-importing the same dump is
/// idempotent per puzzle (`lichess_id` is unique; duplicates are counted
/// and skipped, and theme counts are not double-counted).
pub fn import_puzzles_csv(
    conn: &Connection,
    source: &SourceInfo,
    reader: impl BufRead,
    opts: &PuzzleImportOptions,
) -> anyhow::Result<PuzzleImportStats> {
    let started = Instant::now();
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

    let mut st = PuzzleImportStats::default();
    let mut theme_counts: HashMap<String, i64> = HashMap::new();
    let mut in_batch: u64 = 0;

    conn.execute_batch("BEGIN;")?;
    {
        let mut insert = conn.prepare_cached(
            "INSERT OR IGNORE INTO puzzles
                 (source_id, lichess_id, fen, moves, rating, rating_deviation,
                  popularity, nb_plays, themes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() || (line_no == 0 && line.starts_with("PuzzleId,")) {
                continue;
            }
            let Some(row) = parse_csv_row(&line) else {
                st.malformed += 1;
                continue;
            };
            if opts.min_popularity.is_some_and(|min| row.popularity < min) {
                st.filtered_out += 1;
                continue;
            }
            let inserted = insert.execute(params![
                source_id,
                row.id,
                row.fen,
                row.moves,
                row.rating,
                row.rating_deviation,
                row.popularity,
                row.nb_plays,
                row.themes
            ])?;
            if inserted == 0 {
                st.duplicates_skipped += 1;
                continue;
            }
            st.imported += 1;
            for theme in row.themes.split_whitespace() {
                *theme_counts.entry(theme.to_string()).or_default() += 1;
            }
            in_batch += 1;
            if in_batch >= IMPORT_BATCH {
                conn.execute_batch("COMMIT; BEGIN;")?;
                in_batch = 0;
            }
            if opts.max_rows.is_some_and(|max| st.imported >= max) {
                break;
            }
        }
    }
    for (theme, n) in &theme_counts {
        conn.execute(
            "INSERT INTO puzzle_themes (theme, puzzles) VALUES (?1, ?2)
             ON CONFLICT(theme) DO UPDATE SET puzzles = puzzles + ?2",
            params![theme, n],
        )?;
    }
    conn.execute_batch("COMMIT;")?;
    st.elapsed = started.elapsed();
    Ok(st)
}

// ---------------------------------------------------------------------------
// Puzzle rows and theme list
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleRow {
    pub id: i64,
    pub lichess_id: String,
    pub fen: String,
    /// UCI line; index 0 is the opponent's setup move.
    pub moves: Vec<String>,
    pub rating: i64,
    pub popularity: i64,
    pub themes: Vec<String>,
}

pub fn load_puzzle(conn: &Connection, id: i64) -> anyhow::Result<PuzzleRow> {
    conn.query_row(
        "SELECT id, lichess_id, fen, moves, rating, popularity, themes
         FROM puzzles WHERE id = ?1",
        [id],
        |r| {
            Ok(PuzzleRow {
                id: r.get(0)?,
                lichess_id: r.get(1)?,
                fen: r.get(2)?,
                moves: r
                    .get::<_, String>(3)?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                rating: r.get(4)?,
                popularity: r.get(5)?,
                themes: r
                    .get::<_, String>(6)?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
            })
        },
    )
    .map_err(Into::into)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeCount {
    pub theme: String,
    pub puzzles: i64,
}

/// Theme tags present in the imported set, most frequent first (maintained
/// at import time; never scans the puzzles table).
pub fn theme_list(conn: &Connection) -> anyhow::Result<Vec<ThemeCount>> {
    let mut stmt =
        conn.prepare("SELECT theme, puzzles FROM puzzle_themes ORDER BY puzzles DESC, theme")?;
    let rows = stmt.query_map([], |r| {
        Ok(ThemeCount {
            theme: r.get(0)?,
            puzzles: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn puzzle_count(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM puzzles", [], |r| r.get(0))?)
}

// ---------------------------------------------------------------------------
// Tactics rating (Elo-lite; see module docs)
// ---------------------------------------------------------------------------

pub const RATING_START: f64 = 1500.0;
/// Provisional K while `attempts_before < PROVISIONAL_ATTEMPTS`.
pub const K_PROVISIONAL: f64 = 40.0;
pub const K_ESTABLISHED: f64 = 20.0;
pub const PROVISIONAL_ATTEMPTS: u32 = 30;
pub const RATING_FLOOR: f64 = 500.0;
pub const RATING_CEIL: f64 = 3200.0;

/// Pure Elo update (documented in the module docs). `attempts_before` is
/// the number of rated attempts already on record.
pub fn elo_update(rating: f64, attempts_before: u32, puzzle_rating: f64, solved: bool) -> f64 {
    let k = if attempts_before < PROVISIONAL_ATTEMPTS {
        K_PROVISIONAL
    } else {
        K_ESTABLISHED
    };
    let expected = 1.0 / (1.0 + 10f64.powf((puzzle_rating - rating) / 400.0));
    let score = if solved { 1.0 } else { 0.0 };
    (rating + k * (score - expected)).clamp(RATING_FLOOR, RATING_CEIL)
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticsRating {
    pub rating: f64,
    /// Rated attempts recorded (drives the provisional-K schedule).
    pub attempts: u32,
}

pub fn tactics_rating(conn: &Connection) -> anyhow::Result<TacticsRating> {
    conn.query_row(
        "SELECT rating, attempts FROM tactics_rating WHERE id = 1",
        [],
        |r| {
            Ok(TacticsRating {
                rating: r.get(0)?,
                attempts: r.get(1)?,
            })
        },
    )
    .map_err(Into::into)
}

/// Attempt modes. Only the first three move the rating.
pub const RATED_MODES: [&str; 3] = ["rated", "motif", "weakness"];
pub const ALL_MODES: [&str; 5] = ["rated", "motif", "weakness", "woodpecker", "speed"];

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptOutcome {
    pub rating_before: f64,
    pub rating_after: f64,
    pub attempts: u32,
}

/// Record one attempt in history and (for rating-affecting modes) update
/// the user's tactics rating against the puzzle's fixed rating.
pub fn record_attempt(
    conn: &Connection,
    puzzle_id: i64,
    solved: bool,
    time_ms: i64,
    mode: &str,
    cycle_id: Option<i64>,
) -> anyhow::Result<AttemptOutcome> {
    if !ALL_MODES.contains(&mode) {
        anyhow::bail!("unknown attempt mode {mode:?}");
    }
    let cur = tactics_rating(conn)?;
    conn.execute(
        "INSERT INTO puzzle_attempts
             (puzzle_id, solved, time_ms, rating_at_attempt, mode, cycle_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            puzzle_id,
            solved as i64,
            time_ms,
            cur.rating,
            mode,
            cycle_id
        ],
    )?;
    if !RATED_MODES.contains(&mode) {
        return Ok(AttemptOutcome {
            rating_before: cur.rating,
            rating_after: cur.rating,
            attempts: cur.attempts,
        });
    }
    let puzzle_rating: i64 = conn.query_row(
        "SELECT rating FROM puzzles WHERE id = ?1",
        [puzzle_id],
        |r| r.get(0),
    )?;
    let new = elo_update(cur.rating, cur.attempts, puzzle_rating as f64, solved);
    conn.execute(
        "UPDATE tactics_rating
         SET rating = ?1, attempts = attempts + 1, updated_at = datetime('now')
         WHERE id = 1",
        params![new],
    )?;
    Ok(AttemptOutcome {
        rating_before: cur.rating,
        rating_after: new,
        attempts: cur.attempts + 1,
    })
}

// ---------------------------------------------------------------------------
// Selection: rated drill / motif filter / speed
// ---------------------------------------------------------------------------

pub const RATED_BAND_START: i64 = 100;
pub const RATED_BAND_STEP: i64 = 100;
pub const RATED_BAND_MAX: i64 = 1000;
/// Heisman speed drill: puzzles at least this far below the user's rating.
pub const SPEED_MARGIN: i64 = 300;
pub const SPEED_DEPTH: i64 = 900;

/// Puzzles the user has already solved test memory, not sight; rated-family
/// selection excludes them (Woodpecker deliberately does the opposite).
const UNSOLVED: &str = "id NOT IN (SELECT puzzle_id FROM puzzle_attempts WHERE solved = 1)";

/// Count + random-offset pick over an indexed rating band; avoids loading
/// candidate id lists into memory on a 5M-row table.
fn pick_in_band(
    conn: &Connection,
    lo: i64,
    hi: i64,
    extra_where: &str,
    theme: Option<&str>,
    rng: &mut Rng,
) -> anyhow::Result<Option<i64>> {
    let theme_filter = match theme {
        Some(_) => " AND instr(' ' || themes || ' ', ' ' || ?3 || ' ') > 0",
        None => "",
    };
    let count_sql = format!(
        "SELECT COUNT(*) FROM puzzles
         WHERE rating BETWEEN ?1 AND ?2 AND {extra_where}{theme_filter}"
    );
    let n: i64 = match theme {
        Some(t) => conn.query_row(&count_sql, params![lo, hi, t], |r| r.get(0))?,
        None => conn.query_row(&count_sql, params![lo, hi], |r| r.get(0))?,
    };
    if n == 0 {
        return Ok(None);
    }
    let off = rng.below(n as u64) as i64;
    let pick_sql = format!(
        "SELECT id FROM puzzles
         WHERE rating BETWEEN ?1 AND ?2 AND {extra_where}{theme_filter}
         ORDER BY id LIMIT 1 OFFSET {off}"
    );
    let id: i64 = match theme {
        Some(t) => conn.query_row(&pick_sql, params![lo, hi, t], |r| r.get(0))?,
        None => conn.query_row(&pick_sql, params![lo, hi], |r| r.get(0))?,
    };
    Ok(Some(id))
}

/// Serve an unsolved puzzle within ±`RATED_BAND_START` of `target`,
/// widening the band by `RATED_BAND_STEP` while starved (up to ±`RATED_BAND_MAX`).
pub fn next_rated(conn: &Connection, target: i64, seed: u64) -> anyhow::Result<Option<PuzzleRow>> {
    next_filtered(conn, target, None, seed)
}

/// Rated-band selection restricted to one Lichess theme tag.
pub fn next_by_theme(
    conn: &Connection,
    target: i64,
    theme: &str,
    seed: u64,
) -> anyhow::Result<Option<PuzzleRow>> {
    next_filtered(conn, target, Some(theme), seed)
}

fn next_filtered(
    conn: &Connection,
    target: i64,
    theme: Option<&str>,
    seed: u64,
) -> anyhow::Result<Option<PuzzleRow>> {
    let mut rng = Rng::new(seed);
    let mut band = RATED_BAND_START;
    loop {
        if let Some(id) = pick_in_band(
            conn,
            target - band,
            target + band,
            UNSOLVED,
            theme,
            &mut rng,
        )? {
            return Ok(Some(load_puzzle(conn, id)?));
        }
        if band >= RATED_BAND_MAX {
            return Ok(None);
        }
        band += RATED_BAND_STEP;
    }
}

/// Heisman speed drill: easy puzzles (rating in
/// [user - SPEED_DEPTH, user - SPEED_MARGIN]) against the clock. Already
/// solved puzzles stay in the pool — speed reps are repetition by design.
pub fn next_speed(conn: &Connection, user: i64, seed: u64) -> anyhow::Result<Option<PuzzleRow>> {
    let mut rng = Rng::new(seed);
    match pick_in_band(
        conn,
        user - SPEED_DEPTH,
        user - SPEED_MARGIN,
        "1=1",
        None,
        &mut rng,
    )? {
        Some(id) => Ok(Some(load_puzzle(conn, id)?)),
        // Low-rated user: fall back to the easiest slice available.
        None => match pick_in_band(conn, 0, user, "1=1", None, &mut rng)? {
            Some(id) => Ok(Some(load_puzzle(conn, id)?)),
            None => Ok(None),
        },
    }
}

// ---------------------------------------------------------------------------
// Weakness-weighted selection (the differentiator)
// ---------------------------------------------------------------------------

/// Lichess theme tag → silman motif kind. The motif kinds are the Debug
/// names of `silman_core::record::AlertKind`, which is exactly what the
/// silman-profile motif matrix uses as row keys, so profile rows join
/// directly against this table.
///
/// Kept deliberately tight: only tags whose training value clearly maps to
/// a detector class. Mate-pattern tags (backRankMate, smotheredMate, …)
/// always co-occur with the umbrella "mate" tag, which is mapped, so they
/// are not listed individually.
pub const THEME_MOTIF_MAP: &[(&str, &str)] = &[
    // Loose-piece / LPDO class → Undefended.
    ("hangingPiece", "Undefended"),
    ("fork", "Undefended"),
    ("discoveredAttack", "Undefended"),
    ("skewer", "Undefended"),
    // Defender-removal / overload class → InadequatelyDefended.
    ("pin", "InadequatelyDefended"),
    ("deflection", "InadequatelyDefended"),
    ("attraction", "InadequatelyDefended"),
    ("capturingDefender", "InadequatelyDefended"),
    ("interference", "InadequatelyDefended"),
    ("xRayAttack", "InadequatelyDefended"),
    // Trapped pieces.
    ("trappedPiece", "TrappedPiece"),
    // King-safety class → WeakKing.
    ("exposedKing", "WeakKing"),
    ("kingsideAttack", "WeakKing"),
    ("queensideAttack", "WeakKing"),
    ("attackingF2F7", "WeakKing"),
    ("doubleCheck", "WeakKing"),
    ("mate", "WeakKing"),
];

fn motif_of_theme(theme: &str) -> Option<&'static str> {
    THEME_MOTIF_MAP
        .iter()
        .find(|(t, _)| *t == theme)
        .map(|(_, m)| *m)
}

/// Human phrase for a motif kind, used in the "why this puzzle" text.
fn motif_human(kind: &str) -> &'static str {
    match kind {
        "Undefended" => "loose-piece (undefended / LPDO)",
        "InadequatelyDefended" => "under-defended-piece",
        "TrappedPiece" => "trapped-piece",
        "WeakKing" => "exposed-king",
        _ => "tactical",
    }
}

/// One motif row of the user's profile, as fed by the caller (the app
/// passes `PlayerProfile::motifs` counts through unchanged).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotifWeight {
    /// AlertKind Debug name ("Undefended", "WeakKing", …).
    pub kind: String,
    /// Times the user's own moves created this weakness against them.
    pub allowed: u32,
    /// Exploitable enemy weaknesses the user failed to take.
    pub missed: u32,
}

/// Weight model (documented): motif pressure p = 2*allowed + missed —
/// `allowed` dominates per the product requirement (weaknesses the user's
/// own games keep allowing get drilled hardest). Pressures are normalized
/// by the maximum across motifs; a puzzle's selection weight is
///   1 + WEIGHT_BOOST * max(normalized pressure over its mapped themes)
/// so puzzles training the worst motif are picked ~(1+WEIGHT_BOOST)x as
/// often as unmapped puzzles.
pub const WEIGHT_BOOST: f64 = 4.0;
/// Candidate pool per pick; the band widens until at least this many
/// unsolved candidates are in range (or the band limit is hit).
pub const WEAKNESS_POOL: i64 = 512;
const WEAKNESS_BAND_START: i64 = 150;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaknessChoice {
    pub puzzle: PuzzleRow,
    /// Dominant profiled motif this puzzle trains (None when the puzzle's
    /// themes map to no profiled weakness — the base-weight case).
    pub motif: Option<String>,
    pub allowed: u32,
    pub missed: u32,
    pub matched_themes: Vec<String>,
    /// Selection weight this puzzle carried (1.0 = unweighted baseline).
    pub weight: f64,
    /// UI-ready explanation of WHY this puzzle was chosen.
    pub reason: String,
}

/// Serve a puzzle near `target`, weighted toward the user's profiled motif
/// weaknesses. Deterministic for a given (db, target, weights, seed).
pub fn next_weakness_weighted(
    conn: &Connection,
    target: i64,
    weights: &[MotifWeight],
    seed: u64,
) -> anyhow::Result<Option<WeaknessChoice>> {
    let mut rng = Rng::new(seed);
    // Motif pressure, normalized below by the maximum.
    let pressure: HashMap<&str, (f64, u32, u32)> = weights
        .iter()
        .map(|w| {
            (
                w.kind.as_str(),
                (
                    2.0 * w.allowed as f64 + w.missed as f64,
                    w.allowed,
                    w.missed,
                ),
            )
        })
        .collect();
    let max_p = pressure.values().map(|(p, _, _)| *p).fold(0.0f64, f64::max);

    // Candidate pool: expanding unsolved band around the target rating.
    let mut band = WEAKNESS_BAND_START;
    let pool: Vec<(i64, String)> = loop {
        let (lo, hi) = (target - band, target + band);
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM puzzles WHERE rating BETWEEN ?1 AND ?2 AND {UNSOLVED}"),
            params![lo, hi],
            |r| r.get(0),
        )?;
        if n >= WEAKNESS_POOL || band >= RATED_BAND_MAX {
            if n == 0 {
                return Ok(None);
            }
            // Random contiguous window keeps memory bounded on huge bands
            // while staying deterministic under the seed.
            let off = rng.below((n - (n.min(WEAKNESS_POOL)) + 1) as u64) as i64;
            let mut stmt = conn.prepare(&format!(
                "SELECT id, themes FROM puzzles
                 WHERE rating BETWEEN ?1 AND ?2 AND {UNSOLVED}
                 ORDER BY id LIMIT {WEAKNESS_POOL} OFFSET {off}"
            ))?;
            let rows = stmt.query_map(params![lo, hi], |r| Ok((r.get(0)?, r.get(1)?)))?;
            break rows.collect::<Result<Vec<_>, _>>()?;
        }
        band += RATED_BAND_STEP;
    };

    // Weigh candidates by their best mapped motif.
    let weight_of = |themes: &str| -> f64 {
        if max_p <= 0.0 {
            return 1.0;
        }
        let best = themes
            .split_whitespace()
            .filter_map(motif_of_theme)
            .filter_map(|m| pressure.get(m).map(|(p, _, _)| *p / max_p))
            .fold(0.0f64, f64::max);
        1.0 + WEIGHT_BOOST * best
    };
    let weighted: Vec<(i64, &str, f64)> = pool
        .iter()
        .map(|(id, themes)| (*id, themes.as_str(), weight_of(themes)))
        .collect();
    let total: f64 = weighted.iter().map(|(_, _, w)| w).sum();
    let mut x = rng.unit() * total;
    let mut chosen = *weighted.last().expect("pool checked non-empty");
    for &c in &weighted {
        if x < c.2 {
            chosen = c;
            break;
        }
        x -= c.2;
    }

    // Explanation: dominant profiled motif among the chosen puzzle's themes.
    let (id, themes, weight) = chosen;
    let mut best: Option<(&str, f64)> = None;
    let mut matched: Vec<String> = Vec::new();
    for theme in themes.split_whitespace() {
        if let Some(motif) = motif_of_theme(theme) {
            if let Some(&(p, _, _)) = pressure.get(motif) {
                if p > 0.0 {
                    matched.push(theme.to_string());
                    if best.is_none_or(|(_, bp)| p > bp) {
                        best = Some((motif, p));
                    }
                }
            }
        }
    }
    let puzzle = load_puzzle(conn, id)?;
    let choice = match best {
        Some((motif, _)) => {
            let (_, allowed, missed) = pressure[motif];
            WeaknessChoice {
                reason: format!(
                    "picked because your games allow many {} tactics \
                     ({} allowed, {} missed in your profile) — this puzzle's \
                     themes [{}] train that motif",
                    motif_human(motif),
                    allowed,
                    missed,
                    matched.join(", "),
                ),
                puzzle,
                motif: Some(motif.to_string()),
                allowed,
                missed,
                matched_themes: matched,
                weight,
            }
        }
        None => WeaknessChoice {
            reason: if max_p <= 0.0 {
                "no profiled weaknesses yet — served near your rating".to_string()
            } else {
                "served near your rating (its themes match none of your profiled weaknesses)"
                    .to_string()
            },
            puzzle,
            motif: None,
            allowed: 0,
            missed: 0,
            matched_themes: Vec::new(),
            weight,
        },
    };
    Ok(Some(choice))
}

// ---------------------------------------------------------------------------
// Woodpecker cycles + speed stats
// ---------------------------------------------------------------------------

/// Woodpecker sets draw from [target - 200, target + 100]: mostly at or
/// slightly below the user, per the method (sets must be re-solvable fast).
const WOODPECKER_BAND_LO: i64 = 200;
const WOODPECKER_BAND_HI: i64 = 100;
/// Cap on how many candidate ids one set creation loads.
const WOODPECKER_CANDIDATES: i64 = 10_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WoodpeckerSet {
    pub id: i64,
    pub name: String,
    pub size: i64,
    pub cycles: i64,
    pub created_at: String,
}

/// Create a fixed puzzle set of `size` puzzles near `target`, expanding the
/// band downward/upward while starved. Fails if the database cannot supply
/// `size` puzzles at all.
pub fn create_woodpecker_set(
    conn: &Connection,
    name: &str,
    size: u32,
    target: i64,
    seed: u64,
) -> anyhow::Result<i64> {
    anyhow::ensure!(size > 0, "set size must be positive");
    let mut rng = Rng::new(seed);
    let mut widen = 0i64;
    let ids: Vec<i64> = loop {
        let (lo, hi) = (
            target - WOODPECKER_BAND_LO - widen,
            target + WOODPECKER_BAND_HI + widen,
        );
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM puzzles WHERE rating BETWEEN ?1 AND ?2",
            params![lo, hi],
            |r| r.get(0),
        )?;
        if n >= size as i64 || widen >= RATED_BAND_MAX {
            anyhow::ensure!(
                n >= size as i64,
                "only {n} puzzles within ±{} of {target}; need {size}",
                RATED_BAND_MAX
            );
            let window = n.min(WOODPECKER_CANDIDATES);
            let off = rng.below((n - window + 1) as u64) as i64;
            let mut stmt = conn.prepare(
                "SELECT id FROM puzzles WHERE rating BETWEEN ?1 AND ?2
                 ORDER BY id LIMIT ?3 OFFSET ?4",
            )?;
            let rows = stmt.query_map(params![lo, hi, window, off], |r| r.get(0))?;
            let mut ids: Vec<i64> = rows.collect::<Result<_, _>>()?;
            rng.shuffle(&mut ids);
            ids.truncate(size as usize);
            break ids;
        }
        widen += RATED_BAND_STEP;
    };

    conn.execute_batch("BEGIN;")?;
    conn.execute("INSERT INTO woodpecker_sets (name) VALUES (?1)", [name])?;
    let set_id = conn.last_insert_rowid();
    {
        let mut insert = conn.prepare(
            "INSERT INTO woodpecker_set_puzzles (set_id, puzzle_id, position) VALUES (?1, ?2, ?3)",
        )?;
        for (pos, id) in ids.iter().enumerate() {
            insert.execute(params![set_id, id, pos as i64])?;
        }
    }
    conn.execute_batch("COMMIT;")?;
    Ok(set_id)
}

pub fn woodpecker_sets(conn: &Connection) -> anyhow::Result<Vec<WoodpeckerSet>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.created_at,
                (SELECT COUNT(*) FROM woodpecker_set_puzzles p WHERE p.set_id = s.id),
                (SELECT COUNT(*) FROM woodpecker_cycles c WHERE c.set_id = s.id)
         FROM woodpecker_sets s ORDER BY s.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(WoodpeckerSet {
            id: r.get(0)?,
            name: r.get(1)?,
            created_at: r.get(2)?,
            size: r.get(3)?,
            cycles: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Puzzle ids of a set in solve order.
pub fn woodpecker_set_puzzles(conn: &Connection, set_id: i64) -> anyhow::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT puzzle_id FROM woodpecker_set_puzzles WHERE set_id = ?1 ORDER BY position",
    )?;
    let rows = stmt.query_map([set_id], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Start cycle N+1 for the set; returns the cycle id.
pub fn start_woodpecker_cycle(conn: &Connection, set_id: i64) -> anyhow::Result<i64> {
    // Validate the set exists (FK alone gives an opaque error).
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM woodpecker_sets WHERE id = ?1",
            [set_id],
            |r| r.get(0),
        )
        .optional()?;
    anyhow::ensure!(exists.is_some(), "no woodpecker set with id {set_id}");
    conn.execute(
        "INSERT INTO woodpecker_cycles (set_id, cycle_no)
         VALUES (?1, 1 + COALESCE((SELECT MAX(cycle_no) FROM woodpecker_cycles
                                   WHERE set_id = ?1), 0))",
        [set_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn finish_woodpecker_cycle(conn: &Connection, cycle_id: i64) -> anyhow::Result<()> {
    let n = conn.execute(
        "UPDATE woodpecker_cycles SET finished_at = datetime('now')
         WHERE id = ?1 AND finished_at IS NULL",
        [cycle_id],
    )?;
    anyhow::ensure!(n == 1, "cycle {cycle_id} not found or already finished");
    Ok(())
}

/// Per-cycle stats for cycle-over-cycle comparison. All numbers are
/// aggregates over the attempts recorded with that cycle id.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleStats {
    pub cycle_id: i64,
    pub cycle_no: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub attempts: i64,
    pub solved: i64,
    /// solved / attempts, percent, one decimal (0 when no attempts).
    pub accuracy_pct: f64,
    pub total_time_ms: i64,
    /// total_time_ms / attempts (0 when no attempts).
    pub avg_time_ms: i64,
}

pub fn woodpecker_cycle_stats(conn: &Connection, set_id: i64) -> anyhow::Result<Vec<CycleStats>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.cycle_no, c.started_at, c.finished_at,
                COUNT(a.id), COALESCE(SUM(a.solved), 0), COALESCE(SUM(a.time_ms), 0)
         FROM woodpecker_cycles c
         LEFT JOIN puzzle_attempts a ON a.cycle_id = c.id
         WHERE c.set_id = ?1
         GROUP BY c.id
         ORDER BY c.cycle_no",
    )?;
    let rows = stmt.query_map([set_id], |r| {
        let attempts: i64 = r.get(4)?;
        let solved: i64 = r.get(5)?;
        let total_time_ms: i64 = r.get(6)?;
        Ok(CycleStats {
            cycle_id: r.get(0)?,
            cycle_no: r.get(1)?,
            started_at: r.get(2)?,
            finished_at: r.get(3)?,
            attempts,
            solved,
            accuracy_pct: if attempts == 0 {
                0.0
            } else {
                (solved as f64 / attempts as f64 * 1000.0).round() / 10.0
            },
            total_time_ms,
            avg_time_ms: if attempts == 0 {
                0
            } else {
                total_time_ms / attempts
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------------------
// Solve verification (no engine, per CLAUDE.md #6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveVerdict {
    /// The stored solution move.
    Correct,
    /// Not the stored move, but it delivers checkmate — accepted (the
    /// Lichess convention for alternate mates), and it ends the puzzle.
    CorrectAltMate,
    Wrong,
}

/// Parse a UCI move against `board`, accepting both standard castling
/// ("e1g1") and cozy-chess's king-onto-rook form ("e1h1"). Returns the
/// legal cozy-chess move or an error.
pub fn parse_uci(board: &Board, uci: &str) -> Result<Move, String> {
    let mv: Move = uci.parse().map_err(|_| format!("bad UCI move {uci:?}"))?;
    if board.is_legal(mv) {
        return Ok(mv);
    }
    // Standard-form castling: a two-square king move maps to the rook square.
    let stm = board.side_to_move();
    if board.piece_on(mv.from) == Some(Piece::King)
        && board.color_on(mv.from) == Some(stm)
        && (mv.to.file() as i8 - mv.from.file() as i8).abs() == 2
    {
        let rights = board.castle_rights(stm);
        let rook_file = if mv.to.file() > mv.from.file() {
            rights.short
        } else {
            rights.long
        };
        if let Some(file) = rook_file {
            let castle = Move {
                from: mv.from,
                to: Square::new(file, mv.from.rank()),
                promotion: None,
            };
            if board.is_legal(castle) {
                return Ok(castle);
            }
        }
    }
    Err(format!("illegal move {uci:?} in this position"))
}

/// Check one solver move against the stored solution. `fen` is the position
/// the solver faces; `expected_uci` the stored move; `played_uci` the
/// user's move. An unparseable or illegal played move is simply Wrong.
pub fn verify_move(fen: &str, expected_uci: &str, played_uci: &str) -> anyhow::Result<MoveVerdict> {
    let board: Board = fen
        .parse()
        .map_err(|e| anyhow::anyhow!("bad FEN {fen:?}: {e:?}"))?;
    let expected =
        parse_uci(&board, expected_uci).map_err(|e| anyhow::anyhow!("bad solution move: {e}"))?;
    let Ok(played) = parse_uci(&board, played_uci) else {
        return Ok(MoveVerdict::Wrong);
    };
    if played == expected {
        return Ok(MoveVerdict::Correct);
    }
    let mut after = board.clone();
    after
        .try_play(played)
        .map_err(|e| anyhow::anyhow!("legal move failed to play: {e}"))?;
    if after.status() == GameStatus::Won {
        // The side that just moved delivered checkmate.
        return Ok(MoveVerdict::CorrectAltMate);
    }
    Ok(MoveVerdict::Wrong)
}
