# BENCHMARKS.md

## Phase 0 GO/NO-GO: cozy-chess vs shakmaty (2026-07-25)

**Decision: GO.** ROADMAP.md requires cozy-chess within 2x of shakmaty on
movegen and attack queries; cozy-chess is faster than shakmaty on every axis
measured, on identical hardware, same process, same positions.

### Hardware / toolchain

- Apple M1 Max, 10 cores, 64 GB RAM, macOS (Darwin 25.5.0)
- rustc 1.94.1, `--release` with thin LTO, criterion 0.5
- cozy-chess 0.3.x, shakmaty 0.27.3 (exact pins in Cargo.lock)

### Method

Harness: `bench/movegen-bench` (GPL-3.0, outside the BSD crate graph because
it depends on shakmaty). Four positions spanning opening/tactical/quiet/endgame
(startpos, CPW pos 2 "kiwipete", CPW pos 3, a quiet symmetric middlegame).
A unit test (`kernels_agree`) asserts both libraries produce identical move
counts, attacker counts, and perft(3) values before timing, so both sides do
identical work. Shakmaty's strict extra-material validation is relaxed for one
position (`ignore_too_much_material`); this does not affect movegen speed.

### Results (criterion median, per iteration over all 4 positions)

| Benchmark | cozy-chess | shakmaty | cozy/shak |
|---|---|---|---|
| Legal movegen, all 4 positions | 137.3 ns | 491.9 ns | **0.28x (3.6x faster)** |
| Attackers-to-square, all 64 squares × 4 positions | 633.0 ns | 687.8 ns | **0.92x (1.09x faster)** |
| perft(3) kiwipete (movegen + make composite) | 193.4 µs | 352.5 µs | **0.55x (1.8x faster)** |

### Correctness

Perft suite in `silman-core` green: startpos d1–d5, CPW pos 2 d1–d4,
pos 3 d1–d5, pos 4 d1–d4, pos 5 d1–d4, quiet-middlegame d1–d4. The
quiet-middlegame node counts were cross-validated three ways (cozy-chess,
shakmaty, Stockfish 18 `go perft`) after the commonly published CPW pos-6
figures turned out not to match the FEN as transcribed.

Reproduce: `cargo bench -p movegen-bench`, `cargo test -p silman-core`.

## Phase 1 checkpoint: import + position search (2026-07-25)

Same hardware. Corpus: Lichess standard rated 2013-01 (CC0), 121,332 games
in the file; 121,220 imported, 112 duplicates skipped, 0 parse failures,
8,154,858 positions indexed. Database size ≈ 640 MB (SQLite, WAL).

- Import (silman-cli release build, single-threaded): **86.1 s ≈ 1,409
  games/s** including SAN replay, move encoding, ep-normalized hashing and
  position-index inserts.
- Position search (`silman-cli find-fen`, warm cache):

| Query | Hits | Time |
|---|---|---|
| After 1.e4 (worst case tried) | 72,444 | 139 ms |
| Sicilian after 1.e4 c5 | 11,465 | 31 ms |
| Italian after 3...Nf6 | 1,358 | 5.0 ms |
| Position absent from corpus | 0 | 63 µs |

All far under the ROADMAP 1 s bar at this corpus size. The Phase 1 full
acceptance (sub-second on ≥5M games) still needs a ~40x larger corpus; the
index is a plain SQLite B-tree on (position_hash), so growth is roughly
logarithmic per hit plus linear in hits returned.

