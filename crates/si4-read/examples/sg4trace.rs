//! Debug trace of one game's .sg4 decode: raw bytes and decoded moves.
//! Usage: sg4trace <base.si4> <game-index>

use si4_read::Database;

fn main() {
    let mut args = std::env::args().skip(1);
    let base = args.next().expect("base path");
    let idx: usize = args.next().expect("game index").parse().unwrap();
    let path = std::path::Path::new(&base);
    let db = Database::open(path).unwrap();
    let sg4 = std::fs::read(path.with_extension("sg4")).unwrap();
    let entry = &db.entries[idx];
    println!("{:?}", db.game_header(entry).unwrap());
    let start = entry.game_offset as usize;
    let end = start + entry.game_length as usize;
    let record = &sg4[start..end];
    println!("record ({} bytes): {:02x?}", record.len(), record);
    match si4_read::decode_game(record) {
        Ok(g) => {
            let mut board: cozy_chess::Board = g
                .start_fen
                .as_deref()
                .map(|f| f.parse().unwrap())
                .unwrap_or_default();
            let mut sans = Vec::new();
            for (i, mv) in g.moves.iter().enumerate() {
                if g.null_plies.contains(&i) {
                    sans.push("--".to_string());
                    board = board.null_move().unwrap();
                } else {
                    sans.push(format!("{mv}"));
                    board.play(*mv);
                }
            }
            println!("{} plies: {}", g.moves.len(), sans.join(" "));
            println!(
                "nags={} comments={} variations={} comments_text={:?}",
                g.nag_count, g.comment_count, g.variation_count, g.comments
            );
        }
        Err(e) => println!("DECODE ERROR: {e}"),
    }
}
