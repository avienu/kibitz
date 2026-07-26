//! Streaming PGN import with duplicate detection and position indexing.

use std::io::BufRead;
use std::time::{Duration, Instant};

use cozy_chess::Board;
use rusqlite::{params, Connection};

use crate::db::fnv1a64;
use crate::movebin::{ordered_legal_moves, ENCODING_VERSION};
use crate::pgn::{PgnReader, RawGame};
use crate::san::parse_san;

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub origin: String,
    pub license: String,
}

#[derive(Debug, Default)]
pub struct ImportStats {
    pub games_imported: u64,
    pub duplicates_skipped: u64,
    pub games_failed: u64,
    pub positions_indexed: u64,
    pub elapsed: Duration,
    /// First few failure descriptions, for reporting.
    pub failures: Vec<String>,
}

/// A game fully prepared for insertion.
struct PreparedGame {
    white: Option<String>,
    black: Option<String>,
    event: Option<String>,
    site: Option<String>,
    round: Option<String>,
    date: Option<String>,
    result: u8,
    white_elo: Option<i64>,
    black_elo: Option<i64>,
    eco: Option<String>,
    movetext: Vec<u8>,
    position_hashes: Vec<u64>,
    header_sig: u64,
    moves_hash: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    // cozy-chess's FenParseError does not implement std::error::Error.
    #[error("bad start FEN: {0:?}")]
    CustomStart(cozy_chess::FenParseError),
    #[error("null moves are not representable in encoding v1")]
    NullMove,
    #[error("ply {ply}: {msg}")]
    BadMove { ply: usize, msg: String },
}

/// Replay the SAN mainline, producing the binary movetext, the per-ply
/// position hashes, and the duplicate-detection signatures.
fn prepare(game: &RawGame) -> Result<PreparedGame, PrepareError> {
    let mut board = match game.tag("FEN") {
        Some(fen) if game.tag("SetUp") != Some("0") => {
            fen.parse::<Board>().map_err(PrepareError::CustomStart)?
        }
        _ => Board::default(),
    };

    let mut movetext = Vec::with_capacity(game.sans.len());
    let mut hashes = Vec::with_capacity(game.sans.len());
    let mut moves_hash_input = Vec::with_capacity(game.sans.len() * 2);
    for (ply, san) in game.sans.iter().enumerate() {
        if san == "--" || san == "Z0" {
            return Err(PrepareError::NullMove);
        }
        let mv = parse_san(&board, san).map_err(|e| PrepareError::BadMove {
            ply: ply + 1,
            msg: e.to_string(),
        })?;
        let ordered = ordered_legal_moves(&board);
        let idx = ordered
            .iter()
            .position(|&m| m == mv)
            .expect("parse_san returns a legal move");
        movetext.push(idx as u8);
        moves_hash_input.push(mv.from as u8);
        moves_hash_input.push(mv.to as u8);
        board.play(mv);
        hashes.push(crate::hash::position_hash(&board));
    }

    let tag_owned = |k: &str| game.tag(k).map(str::to_string);
    let norm = |k: &str| game.tag(k).unwrap_or("?").trim().to_ascii_lowercase();
    // Header signature: identity fields that survive re-export between tools
    // (player names, date, result). Event/site names vary across sources and
    // would defeat cross-source duplicate detection.
    let header = format!(
        "{}|{}|{}|{}",
        norm("White"),
        norm("Black"),
        norm("Date"),
        game.result.as_u8()
    );

    let elo = |k: &str| {
        game.tag(k)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|&e| e > 0)
    };

    Ok(PreparedGame {
        white: tag_owned("White"),
        black: tag_owned("Black"),
        event: tag_owned("Event"),
        site: tag_owned("Site"),
        round: tag_owned("Round"),
        date: tag_owned("UTCDate").or_else(|| tag_owned("Date")),
        result: game.result.as_u8(),
        white_elo: elo("WhiteElo"),
        black_elo: elo("BlackElo"),
        eco: tag_owned("ECO"),
        header_sig: fnv1a64(header.as_bytes()),
        moves_hash: fnv1a64(&moves_hash_input),
        movetext,
        position_hashes: hashes,
    })
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

    conn.execute(
        "INSERT INTO sources (name, origin, license) VALUES (?1, ?2, ?3)",
        params![source.name, source.origin, source.license],
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

fn insert_game(
    conn: &Connection,
    source_id: i64,
    g: &PreparedGame,
    stats: &mut ImportStats,
) -> anyhow::Result<()> {
    let white_id = intern(conn, "players", g.white.as_deref())?;
    let black_id = intern(conn, "players", g.black.as_deref())?;
    let event_id = intern(conn, "events", g.event.as_deref())?;
    let site_id = intern(conn, "sites", g.site.as_deref())?;

    let mut game_stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO games
           (source_id, white_id, black_id, event_id, site_id, round, date,
            result, white_elo, black_elo, eco, ply_count, encoding_version,
            movetext, header_sig, moves_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
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
        g.eco,
        g.movetext.len() as i64,
        ENCODING_VERSION,
        g.movetext,
        g.header_sig as i64,
        g.moves_hash as i64,
    ])?;
    if inserted == 0 {
        stats.duplicates_skipped += 1;
        return Ok(());
    }
    let game_id = conn.last_insert_rowid();
    let mut pos_stmt = conn.prepare_cached(
        "INSERT INTO positions (position_hash, game_id, ply) VALUES (?1, ?2, ?3)",
    )?;
    for (i, &h) in g.position_hashes.iter().enumerate() {
        pos_stmt.execute(params![h as i64, game_id, (i + 1) as i64])?;
        stats.positions_indexed += 1;
    }
    stats.games_imported += 1;
    Ok(())
}
