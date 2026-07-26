fn main() {
    let fen = "r4rk1/1pp1qppp/p1np1n2/2b1p1b1/2B1P1B1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";
    let cb = movegen_bench::cozy::parse(fen);
    let sp = movegen_bench::shak::parse(fen);
    for d in 1..=4 {
        println!(
            "d{d}: cozy={} shak={}",
            movegen_bench::cozy::perft(&cb, d),
            movegen_bench::shak::perft(&sp, d)
        );
    }
}
