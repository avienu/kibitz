# RUN_REPORT.md — autonomous run, 2026-07-25

Hardware: Apple M1 Max, 10 cores, 64 GB, macOS (Darwin 25.5.0); rustc 1.94.1,
node 25.9.0. History: 6 conventional commits from empty repo to checkpoint.

## Outcome summary

- **Phase 0: complete**, with two acceptance caveats that require a human or
  CI (see "Remaining risk"): Linux execution of the demo, and real .si4 data.
- **cozy-chess benchmark: GO** — faster than shakmaty on every axis measured
  (movegen 3.6x, attack queries 1.09x, perft(3) 1.8x). docs/BENCHMARKS.md.
- **Phase 1: checkpoint reached** — schema + migrations, streaming PGN
  importer with duplicate detection, ep-normalized Zobrist position index,
  and `silman-cli find-fen` answering "which games reached this FEN" with
  timing printed (63 µs miss / 139 ms worst 72k-hit query on a 121k-game,
  8.15M-position corpus).
- Verification: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, 28 workspace tests, license gate — all green locally; the
  same plus vitest (16) and src-tauri tests (9, incl. two against a real
  Stockfish 18) in app/. Perft suite green at documented depths.

## What was built (by commit)

1. `docs:` project charter/architecture/spec/roadmap (pre-existing content).
2. `build:` cargo workspace (4 BSD crates + GPL silman-db + GPL bench),
   per-crate licenses, CI (fmt/clippy/test/license-gate, Linux+macOS),
   `scripts/license_gate.sh`, perft suite in silman-core.
3. `docs:` benchmark results (GO) + cleanroom si4 format notes.
4. `feat(db):` silman-db — migrations (0001), SAN parser/formatter,
   versioned 1-byte move encoding, malformed-tolerant streaming PGN reader,
   importer with dup detection + provenance, position index, query, CLI.
5. `feat(app):` Tauri v2 demo — chessground board, PGN stepping, tokio UCI
   manager (engine off by default; Analyze is explicit), engine-path
   resolution chain, CI app job. Built by a subagent; reviewed, licenses
   merged into docs/LICENSES.md.
6. `feat(si4):` cleanroom .si4/.sn4 spike + synthetic fixture + dump example.

## Key numbers

| Metric | Value |
|---|---|
| Benchmark GO/NO-GO bar (≤2x shakmaty) | cozy-chess **faster** on all 3 axes |
| Corpus import | 121,220 games / 86 s = 1,409 games/s |
| Positions indexed | 8,154,858 (index ≈ 640 MB db total) |
| find-fen: common/typical/miss | 139 ms / 5–31 ms / 63 µs |
| Duplicates detected in corpus | 112 (+1 planted dup in tests) |

## Deviations from the docs, with rationale

- **Benchmark method:** ROADMAP says "vs published shakmaty numbers on same
  hardware" — contradictory; published numbers are from other hardware. I
  benchmarked both libraries live in one harness (`bench/movegen-bench`,
  GPL-3.0, outside `crates/*` so shakmaty never enters the BSD graph, not
  even as a dev-dependency). Strictly stronger comparison.
- **CPW "position 6" perft figures didn't match its FEN** as commonly
  transcribed; expected counts were re-derived and triple-verified
  (cozy-chess = shakmaty = Stockfish 18 `go perft`). Test renamed to say so.
- **Position hash is ep-normalized** (app/silman-db/src/hash.rs):
  cozy-chess hashes a phantom en-passant file after every double push; raw
  hashes would miss FEN queries and split transpositions (empirically:
  Sicilian-after-1.e4-c5 query returned 0 games before, 11,465 after).
  ARCHITECTURE.md says "Zobrist hash" without this detail; the version is
  recorded in the db (`position_hash_version` = 1) and enforced at open.
- **silman-db is a workspace crate under app/** (ARCHITECTURE places the db
  layer in the app; a separate lib crate keeps the CLI and tests fast and
  keeps heavy Tauri deps out of the root workspace). `app/src-tauri` is a
  standalone cargo package (own lockfile) for the same reason.
- **License gate uses `cargo tree`'s license output** rather than installing
  cargo-license in CI — same check, zero extra tooling; still per-BSD-crate,
  still fails on GPL/LGPL/AGPL anywhere in their trees.
- **Move encoding ordering is self-defined** (sort by from/to/promotion),
  not cozy-chess's internal generation order, so the format is specified by
  the rules of chess rather than a library version (ARCHITECTURE requires
  "deterministic legal-move ordering"; this is the strongest form of it).
- **rust-version bumped 1.80 → 1.82** (for `Option::is_none_or`).

## Cleanroom compliance (si4)

si4-read was written exclusively from community documentation, gathered by a
research agent instructed to never open SCID source; the sources and every
known documentation gap are recorded in docs/SI4_FORMAT_NOTES.md. The
si4spec README links to SCID source as "reference decoder" pointers — those
links were not followed. Gaps that can only be resolved empirically are
listed in DECISIONS_NEEDED.md (§4–5), not resolved by guessing.

## Real-data verification (addendum, same day)

The user pointed at `~/Dropbox/chess/` (read-only). Results:

- **All ten real SCID .si4 databases parse with zero unresolvable entries**
  (47,533 game headers total): benoni2/3, brians, fics, friends_games,
  guy_reams, temp_ma, mypages (3,810 personal FICS/CwF games), twictest
  (4,046 TWIC/World Cup 2011 games), league4545 (39,628 ICC league games;
  its .sg4 is missing but header dumping doesn't need it). Spot-checks match
  known reality (Karjakin 2788 at Khanty-Mansiysk 2011, etc.), and a
  byte-level cross-check of benoni2 against its sibling PGN matched exactly —
  including reproducing odd `Site "3"`-style tags present in the source data.
  Rows shown as `? - ?` are genuine empty comment-only games in the file
  (flags say not-deleted, ply 0, comment count 1), not parse errors.
  → Phase 0's "real .si4 headers dump correctly" criterion **passes**.
- **`mygames` is a PGN, not an .si4** (128 KB, 60 OTB games). Imported with
  0 failures into `testdata/corpus/mygames.sqlite` (10.4 ms, 4,279 positions);
  position search over it works (e.g. 24 games reach the Sicilian after
  1.e4 c5, ~100 µs/query). `bigdatabase` has only an .sn4 (no .si4/.sg4) —
  incomplete remnant, unusable.
- `~/Dropbox/chess/` was not modified; all derived artifacts live in the
  repo's git-ignored `testdata/corpus/`.

Still open from real data: full .sg4 movetext decoding (Phase 1) now has
real files to validate against — the DECISIONS_NEEDED.md §5 ambiguities can
be settled empirically using mypages/twictest.

## What a human must do or decide next (prioritized)

1. ~~Provide real data~~ **Done — see addendum.** Remaining real-data gap:
   ChessBase-exported PGN corpora (the run found none) and the full megabase
   (`bigdatabase` is header-less); point silman at them when available.
2. ~~Run the demo app visually~~ **Done** — user confirmed `cd app && npm
   install && npm run tauri dev` works on macOS.
3. ~~Watch CI~~ **Done** — repo at github.com/avienu/silman; first run
   green on all four jobs (rust + app, each on ubuntu-latest and
   macos-latest), so "builds on Linux" is now verified by an actual run.
   Minor: actions/checkout@v4 and setup-node@v4 emit Node-20 deprecation
   notices; bump to @v5 whenever convenient.
4. **Decide the five parked items in DECISIONS_NEEDED.md** — most urgent are
   the PGN annotation-import policy and null-move encoding (both gate the
   full Phase 1 importer).
5. **Scale test**: full Phase 1 acceptance needs sub-second position search
   on ≥5M games; the largest corpus available this run was 121k games. A
   bigger Lichess month (or your megabase) will answer whether SQLite holds
   or RocksDB escalation is needed.
6. Minor: `tools/` holds Stockfish 18 (git-ignored, user-local); the engine
   path is configurable in-app and via SILMAN_STOCKFISH.

## Test/CI status at end of run

- Root workspace: 28 tests green (perft 6, bench kernel 1, silman-db 16,
  si4-read 5); fmt/clippy/license gate green.
- app/: vitest 16 green; src-tauri fmt/clippy green, 9 tests green including
  live-Stockfish integration; debug bundle builds on macOS.
- CI workflow present for both, unexecuted (no remote configured).
