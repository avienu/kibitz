# ARCHITECTURE.md

## System overview

```
┌────────────────────────────  app/ (GPL-3.0)  ────────────────────────────┐
│  React/TS UI (chessground board, game tree, db browser, trainers,       │
│  opponent-prep views, profile reports)                                   │
│                          │ Tauri IPC (commands/events)                   │
│  Rust shell:                                                             │
│   ├─ db: SQLite (games, players, events, positions index, provenance)   │
│   ├─ importers: PGN, .si4, Lichess/chess.com API clients                │
│   ├─ twic: incremental weekly ingester                                  │
│   ├─ engine: UCI subprocess manager + analysis job queue                │
│   └─ bridge: adapters app-types ⇄ crate-types                           │
└───────────────┬──────────────────────────────────────────────────────────┘
                │ (app depends on crates; never the reverse)
┌───────────────▼──────────  crates/ (BSD-3-Clause)  ──────────────────────┐
│  silman-core: board features on cozy-chess                               │
│   ├─ wsui: tactical screen (weak king, undefended, inadequately         │
│   │        defended, trapped/stalemated pieces) → TacticAlert           │
│   ├─ imbalance: minor pieces, pawn structure, files, squares/outposts,  │
│   │        space, material, development, initiative → Imbalance[]       │
│   ├─ motifs: tactical motif taggers (fork, pin, skewer, ...)            │
│   └─ record: versioned FeatureRecord (serde)                            │
│  silman-profile: batch corpus analysis → PlayerProfile                   │
│  silman-verbalize: FeatureRecord → prose (templates; optional LLM)       │
│  si4-read (if cleanroomed): .si4/.sg4/.sn4 → game structs               │
└──────────────────────────────────────────────────────────────────────────┘
External processes: Stockfish (UCI, GPL — arm's length subprocess),
later Maia via Lc0. Syzygy 3-4-5 local via Fathom FFI; 6/7-piece via
Lichess tablebase API (app layer).
```

## Database (app layer; adapt from En Croissant, GPL-compatible)

- SQLite. Tables: games (headers + binary movetext), players, events, sites,
  sources (provenance: origin, license, import date), migrations.
- Move encoding (v2, 2026-07-25): 1 byte per move = index into a fully
  specified deterministic legal-move ordering (0–217; ordering defined by
  the rules of chess, not a library); byte values 249–255 are inline escape
  tokens (NULL_MOVE, COMMENT with varint+UTF-8, NAG, VAR_START/VAR_END,
  END, reserved ESCAPE) so annotations and variations live in the same
  single-pass stream. Version recorded in the db; upgrades are one-shot
  re-encodes, never dual live encodings.
- Position search: 64-bit Zobrist hash → games index. Start as SQLite table
  (position_hash, game_id, ply); escalate to RocksDB only if Phase 1 benchmarks
  miss targets (sub-second position query on ≥5M games).
- Opening tree: aggregate over position index (move, count, W/D/L, avg elo, perf).
- ECO classification at import via bundled CC0 openings dataset.
- Duplicate detection at import: header signature + move-sequence hash.

## Engine manager & job queue (app layer)

- UCI over stdio, tokio process management, engine registry (path, options,
  hash/threads), MultiPV support.
- Job queue: persistent (SQLite-backed) queue of analysis jobs
  {game/position set, engine, limits (depth|nodes|movetime), purpose}.
  Purposes: wsui-confirm (bounded, e.g. nodes-limited), user-analysis,
  batch-annotate, batch-profile. Progress events to UI. Resumable across restarts.
- The Silman flow: silman-core runs statically (microseconds). Only a fired
  TacticAlert (or explicit user request) enqueues a bounded engine job whose
  result is folded back into the FeatureRecord (verified/refuted, PV, score).

## Trainers (app layer, consuming crates)

- Opening SRS: repertoire tree (PGN/Lichess-study import), FSRS scheduling over
  (position → expected move) cards.
- Tactics: Lichess CC0 puzzle DB (bundled), modes: rated drill, motif-filtered,
  Heisman speed drill (simple tactics, time-pressure), Woodpecker (fixed set,
  repeated accelerating cycles). Weakness-targeted selection driven by
  silman-profile output (motifs the user misses most).
- Endgames: curriculum tiers by rating band (Silman Complete Endgame Course
  structure as the organizing model — structure only, no copyrighted text);
  drill-vs-engine from tablebase-won/drawn positions; Fathom for ≤5-piece truth.

## Opponent prep (app layer + silman-profile)

- Player search over local db + on-demand ingestion from Lichess
  (api/games/user/{u}, NDJSON, honor throttles) and chess.com (monthly archives,
  serial requests, descriptive User-Agent).
- Repertoire fingerprint: frequency/score by ECO and by first-N-plies transposition-
  aware (position-hash based, not move-order based), split by color.
- PlayerProfile (from silman-profile batch run): per-phase ACPL, blunder rate by
  motif (missed vs allowed), performance by pawn-structure family, conversion
  rate from ≥+2.0, defensive hold rate from ≤−1.0, later Maia-predictability.
- Prep view: opponent's weakest lines/structures → surface master games in those
  exact positions from the local megabase via position index.

## LLM verbalizer (silman-verbalize optional feature + app-layer client)

- Input: FeatureRecord ONLY. The prompt forbids introducing chess facts not in
  the record. Output validated: every move mentioned must be legal and present
  in the record's PVs/candidates; on violation, fall back to templates.
- Fully offline templated mode is always available and is the default.

## Licensing enforcement

- CI job runs `cargo license` per workspace member; build fails if any
  crates/* member pulls a GPL/AGPL/LGPL dependency.
- docs/LICENSES.md is the human-readable registry.
