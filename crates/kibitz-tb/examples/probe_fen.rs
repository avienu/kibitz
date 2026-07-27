//! Probe a FEN against a directory of Syzygy files.
//!
//! Usage:
//!   cargo run -p kibitz-tb --example probe_fen -- <tb-dir> "<fen>"

use std::path::Path;

use cozy_chess::Board;
use kibitz_tb::{RootProbe, Tablebase};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(fen)) = (args.next(), args.next()) else {
        eprintln!("usage: probe_fen <tb-dir> \"<fen>\"");
        std::process::exit(2);
    };

    let mut tb = match Tablebase::init(Path::new(&dir)) {
        Ok(tb) => tb,
        Err(e) => {
            eprintln!("init failed: {e}");
            std::process::exit(1);
        }
    };
    println!("largest = {} men", tb.largest());

    let board = match Board::from_fen(&fen, false) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("bad FEN: {e}");
            std::process::exit(2);
        }
    };

    match tb.probe_board(&board) {
        Ok(wdl) => println!("wdl  = {wdl:?}"),
        Err(e) => println!("wdl  probe error: {e}"),
    }
    match tb.probe_root_board(&board) {
        Ok(RootProbe::Move(m)) => println!(
            "root = {:?} best {}{}{} dtz {}",
            m.wdl,
            m.from,
            m.to,
            m.promotion.map(|p| format!("={p}")).unwrap_or_default(),
            m.dtz
        ),
        Ok(other) => println!("root = {other:?}"),
        Err(e) => println!("root probe error: {e}"),
    }
}
