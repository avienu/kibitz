# Changelog

All notable changes to Kibitz. Pre-1.0, the project was built in
numbered work runs (full detail with verification numbers in
`RUN_REPORT.md`); 0.1.0 is the first tagged release and collects them,
newest first.

## 0.1.0 — 2026-07-26

First tagged release: the complete application to date — database,
training, explanatory engine, ten-screen UI — plus the packaging and
release pipeline itself.

### Run 8 — packaging & release pipeline

- macOS (.app + .dmg, arm64-first) and Linux (AppImage + .deb) bundle
  configuration; local macOS bundles verified.
- Signing/notarization/stapling scripts (`scripts/release/`) with
  `--dry-run` gating; CI release workflow on tag `v*` with
  secrets-gated signing, draft GitHub releases, and a `from-source`
  fresh-clone job.
- Tauri v2 updater plumbing: GitHub Releases `latest.json` feed,
  Settings → Updates row ("Check for updates" default ON + "Check
  now"), honest "not configured" state until a signing pubkey ships,
  feed logic covered by a mock-fixture test.
- Final bundle identifier `org.kibitzchess.app`; versions aligned at
  0.1.0; CHANGELOG, RELEASING and RELEASE_CHECKLIST docs.

### Run 7 — all ten screens

- All ten round-2 screens built and merged: Home, Database, Profile,
  Opponent prep, Tactics, Opening tree, Position search, Openings SRS,
  Endgames, Settings + Help & tour — on the five shared components the
  pattern budget allows.
- ECO names resolve everywhere the design shows one; batch operations
  gained an estimate-confirm flow that quotes its own measurement
  basis. 506 tests green (232 Rust, 274 frontend).

### Run 6 — the design system, whole

- Old tab UI replaced by the full design-system game view: nav rail
  with live badges, Studio Walnut and Instrument board treatments,
  evidence-overlay language with filled-polygon arrows, bidirectional
  prose⇄board linkage, full keyboard map, job-queue-driven status
  strip.
- Eval-bar states, annotation editing, and resize rulings implemented;
  349 tests green.

### Run 5 — correctness hardening + tablebases

- Mate scores can no longer render as material (full score-matrix
  regression tests); annotations narrate the delta between positions;
  convergent hints synthesize into one ranked composite plan.
- WSUI firing-rule study published in docs/VALIDATION.md (incumbent
  solo rule won; alternatives are config knobs).
- Fathom FFI landed (crates/kibitz-tb): 3-man Syzygy WDL/DTZ answers
  match the Lichess tablebase API exactly.

### Run 4 — player profiling at corpus scale

- Phase 4 profile shipped and run on the full personal corpus
  (crates/kibitz-profile); all four maintainer verdicts fixed with
  regression tests.
- LLM verbalizer (optional, strictly grounded) and UI wiring; five
  acceptance games re-annotated through the full static-screen →
  bounded-engine → verdict fold-back pipeline.

### Run 3 — explanatory engine checkpoint + opponent prep

- Phase 2 complete: opponent-prep view end to end; annotation-editing
  UI shipped.
- Phase 3 checkpoint: WSUI tactical screen + 8 imbalance detectors
  with cited golden tests, FeatureRecord v1, SQLite job queue with the
  engine-off principle asserted in tests, template verbalization.
- First published validation numbers: holdout recall 81.3% / FP 39.2% /
  precision 89.2% (docs/VALIDATION.md).

### Run 2 — cleanroom .si4 import + Phase 1 database

- Cleanroom .sg4 move decoding (crates/si4-read) from community docs
  only — all format gaps resolved empirically and documented;
  7,905/7,905 real games decode with every move legal.
- Full .si4 import; network game-source clients and repertoire
  fingerprint (Phase 2 checkpoint). Phase 1 complete except annotation
  storage/editing (parked on maintainer decisions, closed in run 3).

### Run 1 — foundations

- Phase 0 complete: Rust workspace with the BSD/GPL license boundary
  enforced by crate graph, Tauri shell, board core benchmark
  (cozy-chess GO: movegen 3.6× faster than the alternative), UCI
  engine manager against a real Stockfish.
- Phase 1 checkpoint: SQLite schema + migrations, streaming PGN
  importer with duplicate detection, ep-normalized Zobrist position
  index answering "which games reached this FEN" in 63 µs (miss) on a
  121k-game / 8.15M-position corpus.
