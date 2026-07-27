# LICENSES.md — dependency license registry

Layer rules (CLAUDE.md): `crates/*` is BSD-3-Clause and must be free of
GPL/AGPL/LGPL dependencies; `app/` and `bench/` are GPL-3.0 and may use them.
Every dependency added to the workspace gets a row here at the moment it is
added. Versions are the requested semver requirement; the lockfile pins exact
versions.

## Direct dependencies

| Name | Req. version | License | Layer(s) | Notes |
|---|---|---|---|---|
| cozy-chess | 0.3 | MIT | crates/*, app, bench | Board representation + movegen. License file verified at pin time (MIT, © analog-hors). |
| cozy-chess-types | (transitive of cozy-chess) | MIT | crates/* | |
| serde | 1 | MIT OR Apache-2.0 | crates/*, app | |
| serde_json | 1 | MIT OR Apache-2.0 | crates/*, app | |
| thiserror | 2 | MIT OR Apache-2.0 | crates/*, app | |
| anyhow | 1 | MIT OR Apache-2.0 | app | |
| rusqlite (bundled) | 0.32 | MIT | app | Bundles SQLite (public domain). |
| clap | 4 | MIT OR Apache-2.0 | app | CLI only. |
| tempfile | 3 | MIT OR Apache-2.0 | dev-deps | Tests only. |
| criterion | 0.5 | MIT OR Apache-2.0 | bench (dev) | Benchmarks only. |
| shakmaty | 0.27 | **GPL-3.0-or-later** | bench ONLY | Comparison baseline for the Phase 0 benchmark. Must never appear in crates/* or be shipped. |
| ureq | 2 | MIT OR Apache-2.0 | app | Blocking HTTP for TWIC/Lichess/chess.com ingesters. Network is app-layer only. |
| zip | 2 | MIT | app | TWIC issue archives. |
| insta | 1 | Apache-2.0 | dev-deps | Snapshot tests (kibitz-profile). |
| Fathom (vendored C source) | commit `c9c6fef0dddc05d2e242c183acf5833149ab676d` | MIT | crates/kibitz-tb | Syzygy probing. Vendored verbatim from <https://github.com/jdart1/Fathom> into `crates/kibitz-tb/vendor/fathom/` with its LICENSE file (MIT, © 2013-2018 Ronald de Man, © 2015 basil00, © 2016-2025 Jon Dart — text verified at vendor time). Compiled and statically linked; the crate's SPDX is therefore `BSD-3-Clause AND MIT`. |
| cc | 1 | MIT OR Apache-2.0 | build-deps (kibitz-tb) | Compiles the vendored Fathom C at build time. |

## Evaluated and rejected

| Name | Version checked | License | Would-be layer | Why rejected |
|---|---|---|---|---|
| fsrs (fsrs-rs) | 6.6.1 | BSD-3-Clause (crate itself) | crates/kibitz-srs | Dependency tree pulls `priority-queue` 2.7.0 (`LGPL-3.0-or-later OR MPL-2.0`), which fails `scripts/license_gate.sh` for BSD crates, plus a ~40-crate ML optimizer stack (ndarray, rayon, …) unneeded for scheduling. Verified 2026-07-26 via `cargo add fsrs` + `cargo tree -e normal --format "{p} | {l}"` in a scratch project. FSRS-4.5 scheduling is instead implemented directly in `crates/kibitz-srs` (BSD-3-Clause, serde-only) from the published algorithm description (parameters and formulas are public; see the crate's doc comment). |

## Bundled data

| Name | License | Location | Notes |
|---|---|---|---|
| lichess-org/chess-openings | CC0-1.0 | data/openings/*.tsv | ECO classification dataset; bundled and redistributable per CLAUDE.md ground rules. |
| Lichess puzzle database | CC0-1.0 | testdata/fixtures/puzzles_sample.csv (committed 500-row test fixture); full dump user-imported via `kibitz-cli import-puzzles` (testdata/corpus/, git-ignored) | Tactics trainer dataset from <https://database.lichess.org/#puzzles>; CC0, may be bundled/redistributed. Provenance recorded in the `sources` table on import. |

## app/ npm dependencies (GPL layer)

| Name | Req. version | License | Notes |
|---|---|---|---|
| react / react-dom | 19.2.x | MIT | |
| chessground | 9.2.x | **GPL-3.0-or-later** | Board UI. app layer only. |
| chessops | 0.14.x | **GPL-3.0-or-later** | TS chess rules/PGN. app layer only. |
| @tauri-apps/api | 2.11.x | Apache-2.0 OR MIT | |
| @tauri-apps/cli | 2.11.x | Apache-2.0 OR MIT | dev |
| typescript | 5.9.x | Apache-2.0 | dev |
| vite / @vitejs/plugin-react / vitest | 6.x / 4.x / 3.x | MIT | dev |
| @types/react, @types/react-dom | 19.x | MIT | dev |
| jsdom | 29.x | MIT | dev (component tests) |
| @testing-library/react, @testing-library/dom | 16.x / 10.x | MIT | dev (component tests) |

## app/src-tauri cargo dependencies (standalone GPL package, own Cargo.lock)

| Name | Req. version | License | Notes |
|---|---|---|---|
| tauri | 2.11.x | Apache-2.0 OR MIT | |
| tauri-build | 2.6.x | Apache-2.0 OR MIT | build |
| tokio | 1.x | MIT | UCI subprocess I/O |
| serde / serde_json | 1.x | MIT OR Apache-2.0 | |

## External tools / data (not linked)

| Name | License | Use |
|---|---|---|
| Stockfish | GPL-3.0 | Arm's-length UCI subprocess. Never linked, never bundled in the repo (user-local `tools/`, git-ignored). |

## CI enforcement

`cargo license` is run per `crates/*` member in CI; the build fails if any
GPL/AGPL/LGPL-licensed crate appears in their dependency trees.
| Public Sans (font, bundled woff2) | v21 latin | SIL OFL 1.1 (license bundled) | app (UI asset) |
| Source Serif 4 (font, bundled woff2) | v14 latin | SIL OFL 1.1 (license bundled) | app (UI asset) |
| JetBrains Mono (font, bundled woff2) | v24 latin | SIL OFL 1.1 (license bundled) | app (UI asset) |
