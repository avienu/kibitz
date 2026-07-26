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

## External tools / data (not linked)

| Name | License | Use |
|---|---|---|
| Stockfish | GPL-3.0 | Arm's-length UCI subprocess. Never linked, never bundled in the repo (user-local `tools/`, git-ignored). |

## CI enforcement

`cargo license` is run per `crates/*` member in CI; the build fails if any
GPL/AGPL/LGPL-licensed crate appears in their dependency trees.
