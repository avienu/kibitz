//! Phase 0 GO/NO-GO benchmark: cozy-chess vs shakmaty on identical hardware.
//!
//! Three axes per ROADMAP.md: legal movegen throughput, attack-map queries,
//! and perft(3) as a composite movegen+make workload.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use movegen_bench::{cozy, shak, BENCH_FENS};

fn bench_movegen(c: &mut Criterion) {
    let mut g = c.benchmark_group("movegen_all_positions");
    let cozy_boards: Vec<_> = BENCH_FENS.iter().map(|(_, f)| cozy::parse(f)).collect();
    let shak_boards: Vec<_> = BENCH_FENS.iter().map(|(_, f)| shak::parse(f)).collect();
    g.bench_function("cozy-chess", |b| {
        b.iter(|| -> u64 {
            cozy_boards
                .iter()
                .map(|board| cozy::count_moves(black_box(board)))
                .sum()
        })
    });
    g.bench_function("shakmaty", |b| {
        b.iter(|| -> u64 {
            shak_boards
                .iter()
                .map(|pos| shak::count_moves(black_box(pos)))
                .sum()
        })
    });
    g.finish();
}

fn bench_attacks(c: &mut Criterion) {
    let mut g = c.benchmark_group("attackers_to_all_64_squares");
    let cozy_boards: Vec<_> = BENCH_FENS.iter().map(|(_, f)| cozy::parse(f)).collect();
    let shak_boards: Vec<_> = BENCH_FENS.iter().map(|(_, f)| shak::parse(f)).collect();
    g.bench_function("cozy-chess", |b| {
        b.iter(|| -> u32 {
            cozy_boards
                .iter()
                .map(|board| cozy::attackers_all_squares(black_box(board)))
                .sum()
        })
    });
    g.bench_function("shakmaty", |b| {
        b.iter(|| -> u32 {
            shak_boards
                .iter()
                .map(|pos| shak::attackers_all_squares(black_box(pos)))
                .sum()
        })
    });
    g.finish();
}

fn bench_perft(c: &mut Criterion) {
    let mut g = c.benchmark_group("perft3_kiwipete");
    g.sample_size(50);
    let (_, fen) = BENCH_FENS[1];
    let cb = cozy::parse(fen);
    let sp = shak::parse(fen);
    g.bench_function("cozy-chess", |b| b.iter(|| cozy::perft(black_box(&cb), 3)));
    g.bench_function("shakmaty", |b| b.iter(|| shak::perft(black_box(&sp), 3)));
    g.finish();
}

criterion_group!(benches, bench_movegen, bench_attacks, bench_perft);
criterion_main!(benches);
