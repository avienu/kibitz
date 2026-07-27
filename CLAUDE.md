# CLAUDE.md

Name: **Kibitz** (renamed from the working-era name in run 8; crate names are now stable — do not rename again).

Open-source chess training + database platform: ChessBase-class database, Chessable-class
training, plus a novel explanatory engine ("Kibitz engine") that explains positions in
human terms. Solo maintainer, experienced CTO. macOS primary, Linux/BSD portable.

## Non-negotiable architectural decisions (do not relitigate)

1. **Stack:** Rust core, Tauri v2 shell, React/TypeScript + chessground front end,
   SQLite storage, Stockfish (and later Maia) as UCI subprocesses.
2. **Licensing is a hard boundary, enforced by crate graph:**
   - `crates/*` (original IP): **BSD-3-Clause**. MUST NOT depend on any GPL code
     (no shakmaty, no shakmaty-syzygy, no code ported from SCID or En Croissant).
   - `app/` (Tauri shell): **GPL-3.0**. May depend on the BSD crates, chessground,
     Stockfish, and code adapted from En Croissant (GPL-3.0) with attribution.
   - Dependency direction: app → crates. NEVER crates → app.
   - Every new dependency: check its license before adding. GPL/AGPL deps are
     app-layer only. Record each in docs/LICENSES.md (name, version, license, layer).
3. **Board representation in BSD crates:** `cozy-chess` (MIT — verify license file at
   pin time). Pending Phase 0 benchmark; if it fails, escalate to the maintainer
   before substituting anything.
4. **Tablebases in BSD crates:** Fathom (MIT) via FFI. shakmaty-syzygy is forbidden
   (GPL).
5. **.si4 import:** cleanroom from the community si4spec docs OR implemented inside
   the GPL app layer (porting SCID source is then allowed). Never port SCID code
   into a BSD crate.
6. **The full engine stays OFF by default.** Stockfish runs only when the WSUI
   tactical screen fires, or when the user explicitly requests analysis, or in
   user-initiated batch jobs. This is a product principle, not an optimization.
7. **ChessBase native formats are out of scope.** Migration path is PGN export.
8. **All engine analysis goes through a job queue** (see ARCHITECTURE.md), never
   synchronous ad-hoc engine calls from UI handlers.

## Repository layout

```
/
├── CLAUDE.md
├── docs/                      # ARCHITECTURE.md, KIBITZ_ENGINE_SPEC.md, ROADMAP.md, LICENSES.md
├── crates/                    # BSD-3-Clause workspace members
│   ├── kibitz-core/           # feature detectors, WSUI screen, imbalance extractor, feature records
│   ├── kibitz-profile/        # corpus batch profiling & aggregation (player strengths/weaknesses)
│   ├── kibitz-verbalize/      # templated NL renderer + LLM-verbalizer trait (LLM impl optional feature)
│   └── si4-read/              # ONLY if cleanroomed from si4spec; else this lives in app/
├── app/                       # GPL-3.0 Tauri application
│   ├── src-tauri/             # Rust: db layer, UCI manager, job queue, TWIC ingester, importers
│   └── src/                   # React/TS + chessground UI
└── LICENSE-BSD, LICENSE-GPL, per-crate LICENSE files
```

## Conventions

- Rust edition 2021+, `cargo clippy -- -D warnings`, `cargo fmt` enforced.
- Every detector in kibitz-core ships with unit tests against known FEN positions
  (test names cite the position source, e.g. Jeremy Silman HTRYC examples, classic games).
- Feature records are the contract between all components: versioned serde structs,
  JSON-serializable, schema documented in docs/KIBITZ_ENGINE_SPEC.md. Breaking
  changes bump the record schema version.
- UI text and explanation templates live in data files, not string literals.
- SQLite schema changes only via numbered migrations.
- Conventional commits. One logical change per commit.
- No network calls in kibitz-core or kibitz-profile. Network (TWIC, Lichess,
  chess.com, LLM APIs) is app-layer or kibitz-verbalize's optional LLM feature only.

## External data ground rules

- Lichess puzzle DB and openings dataset: CC0, may be bundled/redistributed.
- TWIC: personal-use download only. NEVER bundle or redistribute TWIC data in the
  repo or releases. Ingester downloads to the user's machine.
- Track provenance of every imported dataset in the db (source, license, date).

## Definition of done for any phase

Acceptance criteria in docs/ROADMAP.md pass, clippy/fmt clean, tests green,
LICENSES.md current, and no GPL dependency has leaked into crates/*
(`cargo license` output checked in CI).
