//! Whole-database .sg4 decode validator.
//!
//! For every game in a real SCID base, decodes the record and checks:
//!   1. decoding succeeds and consumes a well-formed stream,
//!   2. the mainline ply count equals the index entry's ply count,
//!   3. the final position's material matches the index entry's 24-bit
//!      final-material signature (docs/SI4_FORMAT_NOTES.md §1),
//!
//! which cross-validates the move decoding against data SCID wrote.
//!
//! Usage: cargo run -p si4-read --example sg4validate -- <base.si4> [...]

use cozy_chess::{Board, Color, Piece};
use si4_read::{decode_game, Database};

/// Recompute the index's final-material signature from a board.
fn material_signature(board: &Board) -> u32 {
    let count = |color: Color, piece: Piece, cap: u32| -> u32 {
        (board.colored_pieces(color, piece).len()).min(cap)
    };
    let w = |p, cap| count(Color::White, p, cap);
    let b = |p, cap| count(Color::Black, p, cap);
    (w(Piece::Queen, 3) << 22)
        | (w(Piece::Rook, 3) << 20)
        | (w(Piece::Bishop, 3) << 18)
        | (w(Piece::Knight, 3) << 16)
        | (w(Piece::Pawn, 15) << 12)
        | (b(Piece::Queen, 3) << 10)
        | (b(Piece::Rook, 3) << 8)
        | (b(Piece::Bishop, 3) << 6)
        | (b(Piece::Knight, 3) << 4)
        | b(Piece::Pawn, 15)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: sg4validate <base.si4> [...]");
        std::process::exit(2);
    }
    let mut grand_total = 0u64;
    let mut grand_bad = 0u64;
    for base in &args {
        let path = std::path::Path::new(base);
        let db = match Database::open(path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("{base}: cannot open index: {e}");
                continue;
            }
        };
        let sg4 = match std::fs::read(path.with_extension("sg4")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{base}: no .sg4 ({e}); skipping");
                continue;
            }
        };
        let (mut ok, mut bad, mut empty, mut nulls, mut custom) = (0u64, 0u64, 0u64, 0u64, 0u64);
        let mut ply_mismatch = 0u64;
        let mut material_mismatch = 0u64;
        for (i, entry) in db.entries.iter().enumerate() {
            let start = entry.game_offset as usize;
            let end = start + entry.game_length as usize;
            if end > sg4.len() {
                eprintln!("{base} game {i}: record beyond EOF");
                bad += 1;
                continue;
            }
            match decode_game(&sg4[start..end]) {
                Ok(g) => {
                    if entry.ply_count == 0 {
                        empty += 1;
                    }
                    if !g.null_plies.is_empty() {
                        nulls += 1;
                    }
                    if g.start_fen.is_some() {
                        custom += 1;
                    }
                    if g.moves.len() as u16 != entry.ply_count {
                        ply_mismatch += 1;
                        bad += 1;
                        if ply_mismatch <= 3 {
                            eprintln!(
                                "{base} game {i}: decoded {} plies, index says {}",
                                g.moves.len(),
                                entry.ply_count
                            );
                        }
                        continue;
                    }
                    // Replay to the final position for the material check.
                    // Skipped for games with null moves (not replayable via
                    // play()) and for empty games, whose index entries have
                    // been observed to carry a stale default signature.
                    if g.null_plies.is_empty() && entry.ply_count > 0 {
                        let mut board: Board = g
                            .start_fen
                            .as_deref()
                            .map(|f| f.parse().expect("validated FEN"))
                            .unwrap_or_default();
                        for mv in &g.moves {
                            board.play(*mv);
                        }
                        if material_signature(&board) != entry.final_material {
                            material_mismatch += 1;
                            bad += 1;
                            if material_mismatch <= 3 {
                                eprintln!(
                                    "{base} game {i}: material {:#08x} != index {:#08x}",
                                    material_signature(&board),
                                    entry.final_material
                                );
                            }
                            continue;
                        }
                    }
                    ok += 1;
                }
                Err(e) => {
                    bad += 1;
                    if bad <= 5 {
                        eprintln!("{base} game {i}: {e}");
                    }
                }
            }
        }
        grand_total += ok + bad;
        grand_bad += bad;
        println!(
            "{base}: {ok} ok, {bad} bad (ply {ply_mismatch}, material {material_mismatch}) \
             [{empty} empty, {nulls} with nulls, {custom} custom-start]"
        );
    }
    println!("TOTAL: {grand_total} games, {grand_bad} failures");
    if grand_bad > 0 {
        std::process::exit(1);
    }
}
