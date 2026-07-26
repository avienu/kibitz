use cozy_chess::Board;

fn main() {
    let mut b = Board::default();
    b.play("e2e4".parse().unwrap());
    b.play("c7c5".parse().unwrap());
    let parsed_dash: Board = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2"
        .parse()
        .unwrap();
    let parsed_c6: Board = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2"
        .parse()
        .unwrap();
    println!("played ep={:?} hash={}", b.en_passant(), b.hash());
    println!(
        "dash   ep={:?} hash={}",
        parsed_dash.en_passant(),
        parsed_dash.hash()
    );
    println!(
        "c6     ep={:?} hash={}",
        parsed_c6.en_passant(),
        parsed_c6.hash()
    );
    // and a case where ep capture IS possible
    let mut b2 = Board::default();
    for m in ["e2e4", "a7a6", "e4e5", "d7d5"] {
        b2.play(m.parse().unwrap());
    }
    println!("capturable: played ep={:?}", b2.en_passant());
}
