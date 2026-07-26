//! Full .si4 database import: headers via the si4-read index/namebase,
//! moves via the cleanroom .sg4 decoder.
//!
//! Annotation storage is blocked on DECISIONS_NEEDED.md items 1–2
//! (encoding v2): comments/NAGs/variations are counted and reported, not
//! stored; games containing mainline null moves are skipped and counted.

use std::path::Path;
use std::time::Instant;

use cozy_chess::Board;
use rusqlite::Connection;
use si4_read::{decode_game, Database, IndexEntry};

use crate::import::{insert_game, ImportStats, PreparedGame, SourceInfo};

#[derive(Debug, Default)]
pub struct Si4ImportStats {
    pub base: ImportStats,
    /// Games skipped because they are empty (0 plies).
    pub empty_skipped: u64,
    /// Games skipped because their mainline contains null moves
    /// (unrepresentable in movetext encoding v1; DECISIONS_NEEDED.md #2).
    pub null_move_skipped: u64,
    /// Annotations counted but not stored (DECISIONS_NEEDED.md #1).
    pub comments_dropped: u64,
    pub nags_dropped: u64,
    pub variations_dropped: u64,
}

fn prepare_entry(
    db: &Database,
    entry: &IndexEntry,
    sg4: &[u8],
) -> anyhow::Result<(PreparedGame, u32, u32, u32)> {
    let start = entry.game_offset as usize;
    let end = start + entry.game_length as usize;
    let record = sg4
        .get(start..end)
        .ok_or_else(|| anyhow::anyhow!("game record beyond .sg4 EOF"))?;
    let decoded = decode_game(record)?;
    if !decoded.null_plies.is_empty() {
        anyhow::bail!("mainline contains null moves");
    }
    let board: Board = match decoded.start_fen.as_deref() {
        Some(fen) => fen
            .parse()
            .map_err(|e| anyhow::anyhow!("bad start FEN: {e:?}"))?,
        None => Board::default(),
    };
    let (movetext, hashes, moves_hash) =
        PreparedGame::from_moves(&board, &decoded.moves).map_err(|m| anyhow::anyhow!(m))?;

    let header = db.game_header(entry)?;
    let result = match header.result {
        "1-0" => 1,
        "0-1" => 2,
        "1/2-1/2" => 3,
        _ => 0,
    };
    let date = header.date.to_string();
    let none_if_unknown = |s: String| {
        let t = s.trim();
        if t.is_empty() || t == "?" {
            None
        } else {
            Some(t.to_string())
        }
    };
    let header_sig = PreparedGame::header_signature(
        none_if_unknown(header.white.clone()).as_deref(),
        none_if_unknown(header.black.clone()).as_deref(),
        Some(&date),
        result,
    );
    let prepared = PreparedGame {
        white: none_if_unknown(header.white),
        black: none_if_unknown(header.black),
        event: none_if_unknown(header.event),
        site: none_if_unknown(header.site),
        round: none_if_unknown(header.round),
        date: Some(date),
        result,
        white_elo: (header.white_elo > 0).then_some(header.white_elo as i64),
        black_elo: (header.black_elo > 0).then_some(header.black_elo as i64),
        eco_tag: {
            let e = entry.eco_str();
            // The dataset uses plain 3-character codes; SCID's extended
            // suffixes (e.g. "B33f2") keep only their base form as a tag.
            (!e.is_empty()).then(|| e.chars().take(3).collect())
        },
        start_fen: decoded.start_fen.clone(),
        movetext,
        position_hashes: hashes,
        header_sig,
        moves_hash,
    };
    Ok((
        prepared,
        decoded.comment_count,
        decoded.nag_count,
        decoded.variation_count,
    ))
}

/// Import every game of a SCID base (`<base>.si4/.sg4/.sn4`).
pub fn import_si4(
    conn: &Connection,
    source: &SourceInfo,
    base_path: &Path,
) -> anyhow::Result<Si4ImportStats> {
    let start_time = Instant::now();
    let mut stats = Si4ImportStats::default();

    let db = Database::open(base_path)?;
    let sg4 = std::fs::read(base_path.with_extension("sg4"))?;

    crate::eco::ensure_openings(conn)?;
    conn.execute(
        "INSERT INTO sources (name, origin, license) VALUES (?1, ?2, ?3)",
        rusqlite::params![source.name, source.origin, source.license],
    )?;
    let source_id = conn.last_insert_rowid();

    conn.execute_batch("BEGIN")?;
    for (i, entry) in db.entries.iter().enumerate() {
        if entry.ply_count == 0 {
            stats.empty_skipped += 1;
            continue;
        }
        match prepare_entry(&db, entry, &sg4) {
            Ok((prepared, comments, nags, variations)) => {
                stats.comments_dropped += comments as u64;
                stats.nags_dropped += nags as u64;
                stats.variations_dropped += variations as u64;
                insert_game(conn, source_id, &prepared, &mut stats.base)?;
            }
            Err(e) if e.to_string().contains("null moves") => {
                stats.null_move_skipped += 1;
            }
            Err(e) => {
                stats.base.games_failed += 1;
                if stats.base.failures.len() < 20 {
                    stats.base.failures.push(format!("game {i}: {e}"));
                }
            }
        }
        if (i + 1) % 2_000 == 0 {
            conn.execute_batch("COMMIT; BEGIN;")?;
        }
    }
    conn.execute_batch("COMMIT")?;
    stats.base.elapsed = start_time.elapsed();
    Ok(stats)
}
