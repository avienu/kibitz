# Contributing to Kibitz

Thanks for your interest! Kibitz is a young project with a solo maintainer, so
the rules below exist to keep review fast and the architecture sound. Please
read them before opening a PR.

## Ground rules

- **Small PRs.** One logical change per PR, one logical change per commit
  ([Conventional Commits](https://www.conventionalcommits.org/) format:
  `feat(core): …`, `fix(db): …`, `docs: …`). Large drive-by refactors will be
  asked to split.
- **Open an issue first** for anything beyond a small fix, so we can agree on
  the approach before you invest time.
- **No CLA.** Contributions are accepted under the license of the layer they
  touch (inbound = outbound): BSD-3-Clause for `crates/*`, GPL-3.0 for `app/`.
  You keep your copyright.

## The license boundary (non-negotiable)

The repository is split into two license layers, and the split is enforced by
CI, not convention:

- **`crates/*` is BSD-3-Clause and must never depend on GPL/LGPL/AGPL code.**
  No shakmaty, no shakmaty-syzygy, no code ported from SCID or from other GPL
  chess programs. `scripts/license_gate.sh` fails CI if a forbidden license
  appears anywhere in a BSD crate's dependency tree.
- **`app/` is GPL-3.0** and may use GPL dependencies (chessground, chessops)
  and code adapted from GPL projects with attribution.
- **Dependency direction is `app → crates`, never `crates → app`.**
- **Every new dependency needs its license checked before it lands**, and a row
  in [docs/LICENSES.md](docs/LICENSES.md) (name, version, license, layer).
  GPL/AGPL dependencies are app-layer only. When in doubt, ask in the issue
  first — dependency PRs without a LICENSES.md row won't be merged.
- `.si4` reading stays cleanroom (from the community si4spec docs). Never port
  SCID source into a BSD crate.

## The engine-off principle

Stockfish runs **only** when the tactical screen fires, when the user
explicitly asks for analysis, or in user-initiated batch jobs. This is a
product principle, not an optimization — don't add code paths that spin up an
engine implicitly, and route all analysis through the job queue (see
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)), never synchronously from UI
handlers.

## Tests and quality bar

Before pushing:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/license_gate.sh

cd app && npm test && npm run build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Expectations:

- **Every detector in `kibitz-core` ships with unit tests against known FEN
  positions**, with test names citing the source of the position
  (Jeremy Silman's *How to Reassess Your Chess* examples, classic games, …).
- Feature records are versioned serde structs — breaking changes bump the
  schema version and update
  [docs/KIBITZ_ENGINE_SPEC.md](docs/KIBITZ_ENGINE_SPEC.md).
- UI text and explanation templates live in data files, not string literals.
- SQLite schema changes only via numbered migrations (`app/kibitz-db/migrations/`).
- No network calls in `kibitz-core` or `kibitz-profile`. Network code is
  app-layer (or `kibitz-verbalize`'s optional LLM feature) only.

## Data ground rules

- Never commit game databases, TWIC downloads, or personal data. TWIC data is
  personal-use only and must never be bundled or redistributed.
- Test fixtures must be synthetic or from clearly redistributable sources
  (e.g. the CC0 Lichess datasets), and small.

## Reporting bugs and proposing features

Use the issue templates. For security issues, see [SECURITY.md](SECURITY.md) —
please don't open public issues for vulnerabilities.
