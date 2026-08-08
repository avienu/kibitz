# Changelog

All notable changes to Kibitz. Pre-1.0, the project was built in
numbered work runs (full detail with verification numbers in
`RUN_REPORT.md`); 0.1.0 is the first tagged release and collects them,
newest first.

## 0.1.1 — 2026-08-07

A release-surface fix. No engine or UI changes.

### Auto-update now reaches every platform we ship

0.1.0 published a `latest.json` listing exactly one platform,
`linux-x86_64`. macOS and Windows users got a correctly signed installer
and an updater that would never offer them anything. Two silent causes:
Tauri names the macOS updater bundle `Kibitz.app.tar.gz` with no
architecture, so both macOS legs uploaded the same filename and one was
kept; and the feed generator had no Windows branch at all, despite the
Windows updater signature being built and uploaded every time.

**If you already installed 0.1.0 on macOS or Windows, this one upgrade
has to be done by hand** — the version that fixes auto-update cannot
reach you through the auto-update it fixes. Download below, install over
the top, and later versions will arrive on their own.

### Guards, because none of the three existing ones caught it

- The release now refuses to publish if a platform declared in
  `release-targets.json` is missing from the updater feed. The
  declaration sits next to the bundles each target owes, so adding a
  platform means adding it in one place and the pipeline follows.
- Release notes are generated as real download links from the files
  actually collected, and the build fails if a declared platform has
  nothing to link to. 0.1.0's notes rendered filenames as text; the
  maintainer tried to click them.

## 0.1.0 — 2026-08-07

First tagged release. The sections below are the work runs it collects,
newest first; runs 9-11 are recorded only in `RUN_REPORT.md` and are not
written up here.

### Run 12 — long-term plans, and Windows signing

The engine could describe a position well and advise on it badly. It held
a taxonomy of imbalances, and a taxonomy cannot say "first trade the
defender, then land the knight, then press the weakness behind it" — the
order is the lesson. This run gave plans a shape.

- **Maneuvers** (record schema v4): a reroute is now an ordered record —
  piece, origin, waypoints, destination, cost, prerequisites. Previously
  it was a bag of squares that did not even record which piece was
  moving.
- **Schemes**: plans as sequences with prerequisites, one per square, and
  the pieces that want a square divide the labour rather than duplicating
  it — one may trade the defender off so another can settle there. The
  Sveshnikov now reports its own main line: Bc1-g5 to trade the f6
  knight, then Nc3-d5, then press the backward d6 pawn.
- **Routing generalised** off the knight to bishops, rooks, queens and
  kings, hop ceiling 3 → 5, with waypoint safety judged over TIME against
  a new pawn-contact distance map rather than the current attack map.
- **Effective force**: material weighted by how many moves it needs to
  reach the sector in question, because a rook four moves from the fight
  is not defending anything. This is what finally lets the Opera Game be
  annotated the way humans annotate it — "White's initiative has become a
  stampede" while Black is three pawns up.
- **Eleven new plan hints**, including the first Nimzowitschian ones
  (undermining, overprotection) and the trade family (hunt the bishop
  pair, keep your best piece, trade off theirs).
- **The verdict is fitted, not guessed.** Who-stands-better moved out of
  the test harness into `kibitz-core::verdict` and its per-kind weights
  were fitted against decisive master games with a held-out half.
- **UI**: findings group by horizon — NOW / NEXT / LONG-TERM — and the
  leading finding of each survives collapse, so a long-term plan is no
  longer buried behind whatever is urgent.

Measured on the 162-position private book corpus: imbalances 86.1% →
90.9%, plans 71.4% → 74.1%, favors 62.6% → 68.7%, all 14 negative
anchors clean.

### Release pipeline

- **Windows bundles are signed** (Azure Artifact Signing via OIDC, no
  stored credential) and the signature is verified before publishing.
- **The publish guard checks signatures, not just filenames.** It had
  been matching artifacts by name, which passed happily on unsigned
  binaries — the Windows release had been shipping unsigned.
- Signed and unsigned runs no longer look alike when both are green: the
  state is written to the run summary and stated in the release notes.
- TWIC auto-sync backfill converges (independent budget, paced by time
  rather than rationed by count), retries issues recorded with zero
  games, and its log reports what was imported instead of what was
  planned.

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
