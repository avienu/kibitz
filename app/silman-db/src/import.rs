//! Streaming PGN import with duplicate detection and position indexing.

use std::io::BufRead;
use std::time::{Duration, Instant};

use cozy_chess::Board;
use rusqlite::{params, Connection};

use crate::db::fnv1a64;
use crate::movebin::{ordered_legal_moves, Token, ENCODING_VERSION};
use crate::pgn::{PgnReader, RawGame};
use crate::san::parse_san;

/// Provenance kind, ordered by duplicate-resolution priority (decided
/// 2026-07-25): the copy of a game from the highest-priority source is
/// kept; other copies are recorded in the `duplicates` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SourceKind {
    #[default]
    Other,
    Online,
    Twic,
    Personal,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Personal => "personal",
            SourceKind::Twic => "twic",
            SourceKind::Online => "online",
            SourceKind::Other => "other",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "personal" => SourceKind::Personal,
            "twic" => SourceKind::Twic,
            "online" => SourceKind::Online,
            _ => SourceKind::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub origin: String,
    pub license: String,
    pub kind: SourceKind,
}

#[derive(Debug, Default)]
pub struct ImportStats {
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    /// Duplicates whose kept copy was upgraded to a higher-priority
    /// source's headers (see `duplicates` table).
    pub duplicates_upgraded: u64,
    pub games_failed: u64,
    pub positions_indexed: u64,
    pub elapsed: Duration,
    /// First few failure descriptions, for reporting.
    pub failures: Vec<String>,
}

/// A game fully prepared for insertion.
pub(crate) struct PreparedGame {
    pub white: Option<String>,
    pub black: Option<String>,
    pub event: Option<String>,
    pub site: Option<String>,
    pub round: Option<String>,
    pub date: Option<String>,
    pub result: u8,
    pub white_elo: Option<i64>,
    pub black_elo: Option<i64>,
    /// Fallback ECO from the source's own header; the bundled dataset takes
    /// precedence at insert time.
    pub eco_tag: Option<String>,
    pub start_fen: Option<String>,
    pub built: BuiltMovetext,
    pub header_sig: u64,
}

/// Everything derived from a game's token stream that the insert needs.
pub(crate) struct BuiltMovetext {
    pub movetext: Vec<u8>,
    /// Position hashes for ply 0 (start) through the final mainline ply.
    pub position_hashes: Vec<u64>,
    /// For each hash, the index (into the ordered legal moves of that
    /// position) of the mainline move played from it; None at game end and
    /// after a null (the opening tree skips nulls).
    pub next_indices: Vec<Option<u8>>,
    pub moves_hash: u64,
    pub ply_count: u16,
}

impl PreparedGame {
    /// Encode a token stream and derive the mainline index data. Shared by
    /// the PGN and .si4 importers so duplicate detection works across
    /// sources.
    pub(crate) fn build(
        start: &Board,
        tokens: &[crate::movebin::Token],
    ) -> Result<BuiltMovetext, String> {
        use crate::movebin::{encode_tokens, mainline_of, Ply};
        let movetext = encode_tokens(start, tokens).map_err(|e| e.to_string())?;
        let mainline = mainline_of(tokens);

        let mut board = start.clone();
        let mut hashes = Vec::with_capacity(mainline.len() + 1);
        let mut next_indices = Vec::with_capacity(mainline.len() + 1);
        let mut moves_hash_input = Vec::with_capacity(mainline.len() * 2);
        hashes.push(crate::hash::position_hash(&board));
        for ply in &mainline {
            match ply {
                Ply::Move(mv) => {
                    let idx = ordered_legal_moves(&board)
                        .iter()
                        .position(|m| m == mv)
                        .expect("encode_tokens validated legality");
                    next_indices.push(Some(idx as u8));
                    moves_hash_input.push(mv.from as u8);
                    moves_hash_input.push(mv.to as u8);
                    board.play(*mv);
                }
                Ply::Null => {
                    next_indices.push(None);
                    moves_hash_input.push(64);
                    moves_hash_input.push(64);
                    board = board.null_move().expect("encode_tokens validated the null");
                }
            }
            hashes.push(crate::hash::position_hash(&board));
        }
        next_indices.push(None);
        Ok(BuiltMovetext {
            movetext,
            position_hashes: hashes,
            next_indices,
            moves_hash: fnv1a64(&moves_hash_input),
            ply_count: mainline.len() as u16,
        })
    }

    /// Header signature over identity fields that survive re-export between
    /// tools (see DECISIONS_NEEDED.md item 3).
    pub fn header_signature(
        white: Option<&str>,
        black: Option<&str>,
        date: Option<&str>,
        result: u8,
    ) -> u64 {
        let norm = |v: Option<&str>| v.unwrap_or("?").trim().to_ascii_lowercase();
        fnv1a64(format!("{}|{}|{}|{}", norm(white), norm(black), norm(date), result).as_bytes())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    // cozy-chess's FenParseError does not implement std::error::Error.
    #[error("bad start FEN: {0:?}")]
    CustomStart(cozy_chess::FenParseError),
    #[error("ply {ply}: {msg}")]
    BadMove { ply: usize, msg: String },
}

/// Convert a raw PGN token stream into encoding-v2 tokens, replaying every
/// line (variations included) for legality. Null moves while in check
/// truncate the affected line instead of failing the game (decided
/// 2026-07-25, DECISIONS_NEEDED #2); the count is reported via `warnings`.
fn tokens_from_pgn(
    start: &Board,
    pgn_tokens: &[crate::pgn::PgnToken],
) -> Result<(Vec<Token>, u32), PrepareError> {
    use crate::pgn::PgnToken as P;

    struct Level {
        cur: Board,
        before: Option<Board>,
    }
    let mut out = Vec::with_capacity(pgn_tokens.len());
    let mut level = Level {
        cur: start.clone(),
        before: None,
    };
    let mut stack: Vec<Level> = Vec::new();
    let mut truncated_lines = 0u32;
    // When truncating (in-check null or an unattachable variation), skip
    // tokens until the current line ends. depth_to_close = stack depth at
    // which we resume; None = not skipping. Mainline truncation drops
    // everything after the null.
    let mut skip_until_depth: Option<usize> = None;
    let mut skip_nesting = 0u32;
    let mut ply = 0usize;

    for t in pgn_tokens {
        if let Some(resume_depth) = skip_until_depth {
            match t {
                P::VarStart => skip_nesting += 1,
                P::VarEnd => {
                    if skip_nesting == 0 {
                        if resume_depth == usize::MAX {
                            // Whole variation was swallowed (its VAR_START
                            // was never emitted): emit nothing.
                        } else {
                            // Close of the truncated line itself. If the
                            // variation lost ALL its content, drop it
                            // entirely instead of storing an empty `()`.
                            if stack.len() > resume_depth {
                                level = stack.pop().expect("depth checked");
                            }
                            if matches!(out.last(), Some(Token::VarStart)) {
                                out.pop();
                            } else {
                                out.push(Token::VarEnd);
                            }
                        }
                        skip_until_depth = None;
                    } else {
                        skip_nesting -= 1;
                    }
                }
                _ => {}
            }
            continue;
        }
        match t {
            P::San(san) => {
                let mv = parse_san(&level.cur, san).map_err(|e| PrepareError::BadMove {
                    ply: ply + 1,
                    msg: e.to_string(),
                })?;
                out.push(Token::Move(mv));
                level.before = Some(level.cur.clone());
                level.cur.play(mv);
                if stack.is_empty() {
                    ply += 1;
                }
            }
            P::Null => match level.cur.null_move() {
                Some(next) => {
                    out.push(Token::Null);
                    level.before = Some(level.cur.clone());
                    level.cur = next;
                    if stack.is_empty() {
                        ply += 1;
                    }
                }
                None => {
                    truncated_lines += 1;
                    if stack.is_empty() {
                        // Mainline: drop everything after the null.
                        break;
                    }
                    skip_until_depth = Some(stack.len() - 1);
                    skip_nesting = 0;
                }
            },
            P::Nag(n) => out.push(Token::Nag(*n)),
            P::Comment(c) => out.push(Token::Comment(c.clone())),
            P::VarStart => match level.before.clone() {
                Some(branch) => {
                    out.push(Token::VarStart);
                    stack.push(std::mem::replace(
                        &mut level,
                        Level {
                            cur: branch,
                            before: None,
                        },
                    ));
                }
                None => {
                    // Variation with nothing to replace (e.g. at game
                    // start): skip it entirely.
                    truncated_lines += 1;
                    // We did not emit VAR_START, so swallow the matching
                    // VAR_END too: emulate by entering skip mode one level
                    // "virtually" — reuse the machinery with a sentinel.
                    skip_until_depth = Some(usize::MAX);
                    skip_nesting = 0;
                }
            },
            P::VarEnd => {
                if let Some(parent) = stack.pop() {
                    out.push(Token::VarEnd);
                    level = parent;
                }
            }
        }
    }
    Ok((out, truncated_lines))
}

/// Replay the movetext tokens, producing the encoded blob, per-ply position
/// hashes, and the duplicate-detection signatures.
fn prepare(game: &RawGame) -> Result<PreparedGame, PrepareError> {
    let start_fen = match game.tag("FEN") {
        Some(fen) if game.tag("SetUp") != Some("0") => Some(fen.to_string()),
        _ => None,
    };
    let start = match &start_fen {
        Some(fen) => fen.parse::<Board>().map_err(PrepareError::CustomStart)?,
        None => Board::default(),
    };

    let (tokens, _truncated) = tokens_from_pgn(&start, &game.tokens)?;
    let built = PreparedGame::build(&start, &tokens)
        .map_err(|msg| PrepareError::BadMove { ply: 0, msg })?;

    let tag_owned = |k: &str| game.tag(k).map(str::to_string);
    let elo = |k: &str| {
        game.tag(k)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|&e| e > 0)
    };
    let date = tag_owned("UTCDate").or_else(|| tag_owned("Date"));
    let header_sig = PreparedGame::header_signature(
        game.tag("White"),
        game.tag("Black"),
        date.as_deref(),
        game.result.as_u8(),
    );

    Ok(PreparedGame {
        white: tag_owned("White"),
        black: tag_owned("Black"),
        event: tag_owned("Event"),
        site: tag_owned("Site"),
        round: tag_owned("Round"),
        date,
        result: game.result.as_u8(),
        white_elo: elo("WhiteElo"),
        black_elo: elo("BlackElo"),
        eco_tag: tag_owned("ECO"),
        start_fen,
        header_sig,
        built,
    })
}

/// Archived header fields of a duplicate's losing copy.
type DupHeaders = (
    Option<String>, // white
    Option<String>, // black
    Option<String>, // event
    Option<String>, // site
    Option<String>, // round
    Option<String>, // date
    Option<i64>,    // white_elo
    Option<i64>,    // black_elo
);

/// Non-destructive duplicate handling (decided 2026-07-25): record the
/// losing copy in `duplicates`, and if the incoming source outranks the
/// kept game's source (personal > twic > online > other), swap the kept
/// game's descriptive fields and movetext to the incoming copy first.
/// Returns true if the kept copy was upgraded.
fn record_duplicate(
    conn: &Connection,
    incoming_source: i64,
    g: &PreparedGame,
) -> anyhow::Result<bool> {
    let (kept_id, kept_source, kept_kind): (i64, i64, String) = conn.query_row(
        "SELECT g.id, g.source_id, s.kind
         FROM games g JOIN sources s ON s.id = g.source_id
         WHERE g.moves_hash = ?1 AND g.header_sig = ?2",
        params![g.built.moves_hash as i64, g.header_sig as i64],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let incoming_kind: String = conn.query_row(
        "SELECT kind FROM sources WHERE id = ?1",
        [incoming_source],
        |r| r.get(0),
    )?;
    let upgrade =
        SourceKind::from_str_lossy(&incoming_kind) > SourceKind::from_str_lossy(&kept_kind);

    let mut dup_stmt = conn.prepare_cached(
        "INSERT INTO duplicates
           (kept_game_id, source_id, white, black, event, site, round, date,
            white_elo, black_elo)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
    )?;
    if upgrade {
        // Archive the (losing) kept copy's headers, then overwrite.
        let old: DupHeaders = conn.query_row(
            "SELECT wp.name, bp.name, e.name, s.name, g.round, g.date,
                        g.white_elo, g.black_elo
                 FROM games g
                 LEFT JOIN players wp ON wp.id = g.white_id
                 LEFT JOIN players bp ON bp.id = g.black_id
                 LEFT JOIN events e ON e.id = g.event_id
                 LEFT JOIN sites s ON s.id = g.site_id
                 WHERE g.id = ?1",
            [kept_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )?;
        dup_stmt.execute(params![
            kept_id,
            kept_source,
            old.0,
            old.1,
            old.2,
            old.3,
            old.4,
            old.5,
            old.6,
            old.7
        ])?;
        let event_id = intern(conn, "events", g.event.as_deref())?;
        let site_id = intern(conn, "sites", g.site.as_deref())?;
        conn.execute(
            "UPDATE games SET event_id = ?1, site_id = ?2, round = ?3,
                    white_elo = ?4, black_elo = ?5, source_id = ?6,
                    movetext = ?7
             WHERE id = ?8",
            params![
                event_id,
                site_id,
                g.round,
                g.white_elo,
                g.black_elo,
                incoming_source,
                g.built.movetext,
                kept_id
            ],
        )?;
    } else {
        dup_stmt.execute(params![
            kept_id,
            incoming_source,
            g.white,
            g.black,
            g.event,
            g.site,
            g.round,
            g.date,
            g.white_elo,
            g.black_elo
        ])?;
    }
    Ok(upgrade)
}

fn intern(conn: &Connection, table: &str, name: Option<&str>) -> rusqlite::Result<Option<i64>> {
    let Some(name) = name else { return Ok(None) };
    conn.execute(
        &format!("INSERT OR IGNORE INTO {table} (name) VALUES (?1)"),
        [name],
    )?;
    let id = conn.query_row(
        &format!("SELECT id FROM {table} WHERE name = ?1"),
        [name],
        |r| r.get(0),
    )?;
    Ok(Some(id))
}

/// Import every game from `reader`, committing in batches. Malformed games
/// are counted and skipped, never fatal. Returns aggregate statistics.
pub fn import_pgn<R: BufRead>(
    conn: &Connection,
    source: &SourceInfo,
    reader: R,
) -> anyhow::Result<ImportStats> {
    const BATCH: usize = 2_000;
    let start = Instant::now();
    let mut stats = ImportStats::default();

    crate::eco::ensure_openings(conn)?;
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

    // Explicit BEGIN/COMMIT (not rusqlite's Transaction guard) so the batch
    // boundary can sit inside the streaming loop.
    conn.execute_batch("BEGIN")?;
    let mut in_batch = 0usize;
    for item in PgnReader::new(reader) {
        match item.map_err(anyhow::Error::from).and_then(|raw| {
            let prepared = prepare(&raw)
                .map_err(|e| anyhow::anyhow!("game at line {}: {e}", raw.start_line))?;
            insert_game(conn, source_id, &prepared, &mut stats)?;
            Ok(())
        }) {
            Ok(()) => {}
            Err(e) => {
                stats.games_failed += 1;
                if stats.failures.len() < 20 {
                    stats.failures.push(e.to_string());
                }
            }
        }
        in_batch += 1;
        if in_batch >= BATCH {
            conn.execute_batch("COMMIT; BEGIN;")?;
            in_batch = 0;
        }
    }
    conn.execute_batch("COMMIT")?;
    stats.elapsed = start.elapsed();
    Ok(stats)
}

pub(crate) fn insert_game(
    conn: &Connection,
    source_id: i64,
    g: &PreparedGame,
    stats: &mut ImportStats,
) -> anyhow::Result<()> {
    let white_id = intern(conn, "players", g.white.as_deref())?;
    let black_id = intern(conn, "players", g.black.as_deref())?;
    let event_id = intern(conn, "events", g.event.as_deref())?;
    let site_id = intern(conn, "sites", g.site.as_deref())?;

    // ECO: bundled-dataset classification (deepest book position reached)
    // wins; the source's own tag is the fallback.
    let eco = match crate::eco::classify(conn, &g.built.position_hashes[1..])? {
        Some((eco, _name)) => Some(eco),
        None => g.eco_tag.clone(),
    };

    let mut game_stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO games
           (source_id, white_id, black_id, event_id, site_id, round, date,
            result, white_elo, black_elo, eco, ply_count, encoding_version,
            movetext, header_sig, moves_hash, start_fen)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
    )?;
    let inserted = game_stmt.execute(params![
        source_id,
        white_id,
        black_id,
        event_id,
        site_id,
        g.round,
        g.date,
        g.result,
        g.white_elo,
        g.black_elo,
        eco,
        g.built.ply_count as i64,
        ENCODING_VERSION,
        g.built.movetext,
        g.header_sig as i64,
        g.built.moves_hash as i64,
        g.start_fen,
    ])?;
    if inserted == 0 {
        stats.duplicates_skipped += 1;
        if record_duplicate(conn, source_id, g)? {
            stats.duplicates_upgraded += 1;
        }
        return Ok(());
    }
    let game_id = conn.last_insert_rowid();
    let mut pos_stmt = conn.prepare_cached(
        "INSERT INTO positions (position_hash, game_id, ply, next_byte)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    // Row `ply` holds the position after `ply` mainline plies (ply 0 =
    // start) and the ordered-legal-move index of the move played FROM it
    // (NULL at game end and after a null move).
    for (ply, &h) in g.built.position_hashes.iter().enumerate() {
        let next_byte = g.built.next_indices[ply].map(|b| b as i64);
        pos_stmt.execute(params![h as i64, game_id, ply as i64, next_byte])?;
        stats.positions_indexed += 1;
    }
    stats.games_imported += 1;
    Ok(())
}
