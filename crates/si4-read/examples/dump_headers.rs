//! Dump every game header of a SCID database to stdout (Phase 0 spike).
//!
//! Usage:
//!   cargo run -p si4-read --example dump_headers -- /path/to/base[.si4]
//!   cargo run -p si4-read --example dump_headers -- --demo   (synthetic db)

use si4_read::fixture::{build_si4, build_sn4, FixtureGame};
use si4_read::Database;

fn demo_db() -> Database {
    let games = [
        FixtureGame {
            white_id: 0,
            black_id: 1,
            event_id: 0,
            site_id: 0,
            round_id: 0,
            result: 1,
            eco: "C41",
            date: (1858, 11, 2),
            white_elo: 0,
            black_elo: 0,
            ply_count: 33,
        },
        FixtureGame {
            white_id: 1,
            black_id: 0,
            event_id: 0,
            site_id: 0,
            round_id: 1,
            result: 3,
            eco: "B90",
            date: (1858, 11, 3),
            white_elo: 0,
            black_elo: 0,
            ply_count: 61,
        },
    ];
    let si4 = build_si4("demo database", &games);
    let sn4 = build_sn4(
        &["Morphy, Paul", "Duke Karl / Count Isouard"],
        &["Paris Opera"],
        &["Paris FRA"],
        &["1", "2"],
    );
    Database::from_bytes(&si4, &sn4).expect("demo fixture parses")
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: dump_headers <base.si4 | --demo>");
        std::process::exit(2);
    });
    let db = if arg == "--demo" {
        demo_db()
    } else {
        match Database::open(std::path::Path::new(&arg)) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    };

    println!(
        "# {} — {} game(s), si4 v{}",
        db.header.description, db.header.num_games, db.header.version
    );
    for entry in &db.entries {
        match db.game_header(entry) {
            Ok(g) => println!(
                "{} - {}  {}  ({})  R{}  {}  {}  elo {}/{}  {} plies",
                g.white,
                g.black,
                g.event,
                g.site,
                g.round,
                g.date,
                g.result,
                g.white_elo,
                g.black_elo,
                g.ply_count
            ),
            Err(e) => println!("<unresolvable entry: {e}>"),
        }
    }
}
