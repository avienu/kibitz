# RUN_REPORT.md

# Run 2 — 2026-07-25 (later the same day)

## ⚠ Headline: Phase 1 is complete EXCEPT annotation storage/editing

Per this run's instructions, parked decisions that block acceptance were
NOT decided. DECISIONS_NEEDED.md items 1–2 (annotation storage / encoding
v2, null moves) gate exactly two Phase 1 deliverables: storing si4/PGN
annotations and the annotation-EDITING half of the game view. Everything
else in Phase 1 is built and verified; the game browser is read-only. The
sg4 decoder already extracts all annotations (16,653 comments / 29,576
NAGs / 8,680 variations counted in your two big SCID bases alone), so once
encoding v2 is blessed, storage is a contained change.

**Phase 2 checkpoint: reached** (network clients + repertoire fingerprint).

## Step 0 — verification of run 1's claims

All of run 1's claims reproduced: fmt/clippy/license-gate green, 28+9
tests green, perft green, position search 30 ms warm / 64 µs miss on the
121k corpus. One nuance the run-1 report under-stated: its find-fen
timings were warm-cache; a cold first query on the 11k-hit Sicilian is
~315 ms. No other discrepancies.

## Built this run

1. **Cleanroom .sg4 move decoding** (crates/si4-read): all documented
   format gaps resolved empirically against your own databases — the
   community doc's rook table is transposed; pawn double-push is code 15;
   piece numbers are swap-remove list indices; null move is byte 0x00;
   variation markers branch from before the previous move; tags ≥0xF1 are
   coded. Findings recorded in docs/SI4_FORMAT_NOTES.md §6 with the
   validation protocol. **7,905/7,905 real games decode** with every move
   legal, ply counts matching the index, and final material matching the
   index signature. No SCID source consulted at any point.
2. **Full .si4 import** (`import-si4`): mypages + twictest = 7,786 games
   in 3.9 s, zero failures; shared insert path with the PGN importer so
   duplicate detection is cross-source; dropped annotations counted and
   reported to the user at import time.
3. **ECO tagging** via the bundled CC0 lichess openings dataset
   (data/openings/, in LICENSES.md), deepest-book-position match by
   position hash (transposition-aware); source tag as fallback.
4. **Opening tree** (`opening-tree`): per-move games/W/D/L/avg-elo/perf
   from the position index (ply-0 rows + next-move byte, migration 0002/3).
   Hand-validated on a fixture; sane on real data (master vs blitz
   segments separate cleanly). 22 ms typical, 956 ms worst-case (startpos).
5. **TWIC ingester**: incremental, provenance-tagged, donation notice,
   explicit --from on first run, 404 stop, no TWIC data in repo. Live
   verification: issue 1650 = 11,027/11,027 games after the Latin-1 fix.
6. **PGN encoding fix**: the reader now decodes UTF-8-else-Latin-1 per
   line. Before the fix TWIC 1650 lost 420 games AND imported ~420
   header-less fragments (run-1's reader was silently wrong on the PGN
   spec's own canonical encoding — worth remembering as a class of bug).
7. **Lichess + chess.com clients**: serial, resumable (since / month
   cursors in meta), 429 backoff, descriptive UA with PLACEHOLDER_EMAIL
   for you to fill in. Offline fixture tests (29); live tests env-gated
   behind SILMAN_NET_TESTS=1 and run exactly once each during the build.
8. **ICC & FICS** (user-requested): ICC has no scriptable HTTP surface
   (SPA + websocket protocol; documented stub with manual PGN-export
   path). FICS implemented via ficsgames.org download CGI with
   documented usage caveats (DECISIONS_NEEDED #7).
9. **PGN export** (`export-pgn`) + verification-bar tests: import→export→
   reparse semantic equality on an annotated Latin-1 fixture (mainline +
   headers; annotations excluded per parked decisions), export-reimport
   dedup, and TWIC-vs-personal duplicate collision tests including a
   same-players-same-day negative case.
10. **Repertoire fingerprint** (Phase 2 checkpoint): pure aggregation in
    silman-profile (insta-snapshot + property tests), db adapter + CLI.
    Real output for `sounix` (1,031 games): 58.4% as White on 1.e4 with
    Caro-Kann B12 at 73.7% and Modern B06 at 29.4%; Sicilian repertoire
    as Black; deviation points with example games. Transposition-aware by
    position hash, split by color, per-ECO scores, first-book-exit
    deviations — exactly the shape the Phase 2 prep view needs.
11. **Game browser UI**: see addendum below (built by a subagent in
    parallel; status recorded when its verification completed).

## Numbers (details in docs/BENCHMARKS.md, run-2 section)

| Metric | Value |
|---|---|
| sg4 decode validation | 7,905/7,905 real games |
| si4 import (7,786 games) | 3.9 s |
| TWIC 1650 live ingest (11,027 games) | ~8 s, 0 failures |
| 121k-corpus reimport with ECO | 111 s (1,093 games/s) |
| Opening tree typical / worst | 22 ms / 956 ms (startpos) |
| Fingerprint (1,031 games) | ~0.5 s |

## Deviations / notes

- ROADMAP says si4 import covers "annotations where representable" —
  nothing is representable in encoding v1, so mainline-only import is
  technically compliant, but I am not claiming the criterion in spirit;
  it's the headline blocker above.
- Duplicate-detection rule evolution: header signatures now prefer
  UTCDate; 8 games of the Lichess corpus shifted from duplicate to unique
  vs run 1 (dup rule is parked decision #3 — unchanged in substance).
- The fingerprint's "example game" for a deviation shows any database game
  reaching that position (context, not the player's own game) — labeled
  "e.g." in the CLI.
- New non-blocking decisions filed: opening-tree root-latency escalation
  (#6), ficsgames.org usage posture (#7).

## What a human must do or decide next

1. **Decide encoding v2** (DECISIONS_NEEDED 1–2) — the only Phase 1
   blocker. A concrete best-in-world design was discussed and is
   referenced in the file.
2. Fill in `PLACEHOLDER_EMAIL` in app/silman-db/src/net/mod.rs (User-Agent
   contact) before real Lichess/chess.com use.
3. Bless or tune the dup-detection definition (#3) and the ficsgames.org
   posture (#7).
4. Optional: run `silman-cli twic-sync --from <current-issue>` weekly (or
   ask for a scheduled routine); TWIC scheduling inside the app is still
   open.
5. Scale test toward ≥5M games (megabase) — includes the opening-tree
   root-cache decision (#6).

---

# Run 1 — 2026-07-25

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
