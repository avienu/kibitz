//! Item-4 probe: does the top nibble of the .si4 Elo fields ever carry a
//! value in real databases, and if so, what does it correlate with?
//! Usage: eloprobe <base.si4> [...]

use si4_read::Database;

fn main() {
    let mut hist = [0u64; 16];
    let mut samples: Vec<(String, u16, u8)> = Vec::new();
    for base in std::env::args().skip(1) {
        let path = std::path::Path::new(&base);
        let db = match Database::open(path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("{base}: {e}");
                continue;
            }
        };
        // Re-read raw entries to get the unmasked 16-bit Elo fields.
        let bytes = std::fs::read(path.with_extension("si4")).unwrap();
        for (i, entry) in db.entries.iter().enumerate() {
            let off = 182 + i * 47;
            for (which, raw_off) in [("white", off + 29), ("black", off + 31)] {
                let raw = u16::from_be_bytes([bytes[raw_off], bytes[raw_off + 1]]);
                let nibble = (raw >> 12) as u8;
                hist[nibble as usize] += 1;
                if nibble != 0 && samples.len() < 20 {
                    let h = db.game_header(entry).unwrap();
                    let name = if which == "white" { &h.white } else { &h.black };
                    samples.push((format!("{base}#{i} {which} {name}"), raw & 0xFFF, nibble));
                }
            }
        }
    }
    println!("top-nibble histogram (0..15): {hist:?}");
    for (ctx, elo, nib) in samples {
        println!("nibble {nib}: elo {elo} — {ctx}");
    }
}
