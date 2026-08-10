fn main() {
    let fen = std::env::args().nth(1).expect("fen");
    let board: cozy_chess::Board = fen.parse().expect("fen");
    let record = kibitz_core::analyze(&board);
    for s in kibitz_core::suggest::suggest(&record, &board) {
        println!(
            "{} score={} risk={:?} serving={:?}",
            s.san,
            s.score,
            s.static_risk,
            s.serving.iter().take(3).collect::<Vec<_>>()
        );
    }
}
