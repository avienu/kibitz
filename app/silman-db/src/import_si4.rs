//! Full .si4 database import: headers via the si4-read index/namebase,
//! moves via the cleanroom .sg4 decoder.
//!
//! Comments, NAGs and variations are stored inline in movetext encoding
//! v2 (decided 2026-07-25); null moves are stored as tokens. Lines with a
//! null move played while in check are unrepresentable and truncated at
//! that point (counted, never fatal).

use std::path::Path;
use std::time::Instant;

use cozy_chess::Board;
use rusqlite::Connection;
use si4_read::sg4::GameToken;
use si4_read::{decode_game, Database, IndexEntry};

use crate::movebin::Token;

use crate::import::{insert_game, ImportStats, PreparedGame, SourceInfo};

#[derive(Debug, Default)]
pub struct Si4ImportStats {
    pub base: ImportStats,
    /// Games skipped because they are empty (0 plies).
    pub empty_skipped: u64,
    /// Annotations stored inline in encoding v2.
    pub comments_stored: u64,
    pub nags_stored: u64,
    pub variations_stored: u64,
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
    let board: Board = match decoded.start_fen.as_deref() {
        Some(fen) => fen
            .parse()
            .map_err(|e| anyhow::anyhow!("bad start FEN: {e:?}"))?,
        None => Board::default(),
    };
    // Map the sg4 token stream onto encoding-v2 tokens, resolving comment
    // references (missing texts become empty comments, defensively).
    let tokens: Vec<Token> = decoded
        .tokens
        .iter()
        .map(|t| match t {
            GameToken::Move(mv) => Token::Move(*mv),
            GameToken::Null => Token::Null,
            GameToken::Nag(n) => Token::Nag(*n),
            GameToken::Comment(i) => {
                Token::Comment(decoded.comments.get(*i).cloned().unwrap_or_default())
            }
            GameToken::VarStart => Token::VarStart,
            GameToken::VarEnd => Token::VarEnd,
        })
        .collect();
    let built = PreparedGame::build(&board, &tokens).map_err(|m| anyhow::anyhow!(m))?;

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
        header_sig,
        built,
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
        "INSERT INTO sources (name, origin, license, kind) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            source.name,
            source.origin,
            source.license,
            source.kind.as_str()
        ],
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
                stats.comments_stored += comments as u64;
                stats.nags_stored += nags as u64;
                stats.variations_stored += variations as u64;
                insert_game(conn, source_id, &prepared, &mut stats.base)?;
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
