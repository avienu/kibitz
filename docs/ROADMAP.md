# ROADMAP.md

Phases are sequential gates. A phase is done when its acceptance criteria pass
(plus the global Definition of Done in CLAUDE.md).

## Phase 0 — Spike (de-risk the stack)

Build:
- Cargo workspace per CLAUDE.md layout; licenses in place; CI (fmt, clippy,
  test, cargo-license gate on crates/*).
- Tauri v2 app showing a chessground board; load a PGN game; step through moves.
- UCI manager: spawn Stockfish, run `go nodes N` on the displayed position,
  show eval + PV live.
- cozy-chess benchmark: perft correctness suite + movegen/attack-map throughput
  vs published shakmaty numbers on same hardware. GO/NO-GO: within 2x of
  shakmaty on movegen and attack queries. If NO-GO, stop and consult maintainer.
- Read one real .si4 file: parse header + index records + name file; dump game
  headers to stdout (spike-quality; cleanroom from si4spec, no SCID source open
  in any window while writing this).

Accept: board+PGN+engine demo runs on macOS and Linux; perft green; benchmark
recorded in docs/; .si4 headers of the user's real database dump correctly.

## Phase 1 — Personal database core (replaces SCID/ChessBase daily use)

Build:
- SQLite schema + migrations; binary move encoding; provenance table.
- PGN importer (streaming, malformed-input tolerant, duplicate detection).
- .si4 importer completed (full games, annotations where representable).
- Zobrist position index; position search UI; opening tree with W/D/L + perf.
- ECO tagging via CC0 openings dataset (bundled).
- TWIC ingester: incremental fetch of new issues, import, provenance-tagged.
  First-run notice pointing users at TWIC's donation page. No TWIC data in repo.
- Game browser + game view with annotation editing (NAGs, comments, variations).

Accept: user's full SCID + exported-ChessBase PGN corpora imported; position
search < 1s on the merged corpus (record size + timing); TWIC auto-ingests a
new week on schedule; duplicates across TWIC/SCID/PGN detected.

## Phase 2 — Opponent prep

Build:
- Lichess + chess.com clients (rate-limit compliant, resumable, provenance-tagged).
- Player pages: repertoire fingerprint (transposition-aware, by color), score by
  ECO family, deviation-from-theory points.
- "Prep view": pick opponent + color → weakest lines → master games in those
  positions from local db.

Accept: end-to-end prep against a real opponent (user picks one) produces a
line-level report and a playable master-game list in < 5 min including download.

## Phase 3 — Silman annotator v1 (templated, offline)

Build:
- silman-core WSUI detectors + imbalance detectors + FeatureRecord (per spec).
- Analysis job queue; WSUI-gated bounded Stockfish confirmation.
- silman-verbalize template mode.
- UI: annotate-this-position panel with board overlays from evidence squares;
  batch-annotate-game job producing inline comments.
- Validation harness per SILMAN_ENGINE_SPEC.md; publish precision/recall.

Accept: validation numbers published; annotating a full game stays within the
engine-off principle (engine runs only on fired screens); golden-file tests
green; the user judges annotations on 5 of his own games "useful, not wrong".

## Phase 4 — Profiling + Silman v2 (LLM verbalizer)

Build:
- silman-profile: PlayerProfile per spec; batch pipeline over corpora.
- Profile report UI (self and opponent); prep view upgraded with profile data.
- LLM verbalizer behind feature flag + app-layer client (user-supplied API key),
  with post-validation and template fallback. Offline mode remains default.

Accept: user's own 10-year corpus profiled; motif-weakness matrix drives at
least one actionable finding the user confirms; LLM mode passes the
validation-fallback tests (inject hallucinated outputs in tests; fallback fires).

## Phase 5 — Trainers

Build:
- Opening SRS: repertoire import (PGN + Lichess studies), FSRS scheduling,
  training UI. Do not use the name "MoveTrainer".
- Tactics: bundle Lichess CC0 puzzles; rated drill, motif filter, Heisman speed
  mode, Woodpecker cycles; weakness-targeted queue from silman-profile.
- Endgames: rating-tiered curriculum structure; drill-vs-engine from
  tablebase positions; Fathom (≤5 piece) local, Lichess API for 6-7 piece.

Accept: daily-training loop works offline (except optional online tablebase);
weakness-targeted tactics queue demonstrably weights the user's worst motifs.

## Deferred (explicitly out of scope until post-Phase 5)

Cloud sync; mobile; collaborative studies; video/lesson media catalog
(timestamps↔FEN links); Maia integration (sparring + predictability profiling);
public binary releases/packaging polish; ChessBase-format personal converter
(separate private project if ever).
