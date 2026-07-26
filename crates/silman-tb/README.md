# silman-tb

Syzygy endgame tablebase probing (WDL + root DTZ) for silman, wrapping the
vendored [Fathom](https://github.com/jdart1/Fathom) C probing library via a
small hand-written FFI layer. No bindgen.

## Licensing

- Crate code (`src/`, `build.rs`, tests): **BSD-3-Clause** (see `LICENSE`).
- Vendored C code (`vendor/fathom/`): **MIT**, © 2013–2018 Ronald de Man,
  © 2015 basil00, © 2016–2025 Jon Dart. The upstream `LICENSE` file is
  preserved verbatim at `vendor/fathom/LICENSE` and was verified to be the MIT
  license text at vendor time.
- Package SPDX expression: `BSD-3-Clause AND MIT` (the compiled artifact
  statically links the MIT C code).

This crate contains **no GPL code** and must never gain a GPL dependency
(`scripts/license_gate.sh` checks it in CI). shakmaty-syzygy is GPL and is
forbidden per CLAUDE.md; Fathom is the designated tablebase path.

## Vendored Fathom provenance

- Upstream: <https://github.com/jdart1/Fathom>
- Commit: `c9c6fef0dddc05d2e242c183acf5833149ab676d` (master, fetched
  2026-07-26 via
  `https://github.com/jdart1/Fathom/archive/c9c6fef0dddc05d2e242c183acf5833149ab676d.tar.gz`)
- Files vendored verbatim from `src/` (plus the top-level `LICENSE`), sha256:

| File | sha256 |
|---|---|
| LICENSE | `c8038055839bd02f995cd7d1bba19657720097f5604411a7977ebcb5b4a37759` |
| tbprobe.c | `7ed0cc80626271342bfe413d6c421756b6851f977a064461f5f555e1b598df35` |
| tbprobe.h | `dc7808dc58c2a1af921a612f94041b917dfac94d3215a76a2b2b0c53c94ea561` |
| tbconfig.h | `69fe1821b471164bd759cf46c486d55b7fa727e348a5ee306b18d19537ac3a03` |
| stdendian.h | `e5b8187eec89ef83e731ea92be8ca0df4b533d8d21075fc4e2c86b4b00d7b7c3` |
| tbchess.c | `119f19fa8714686798ca7be011e5b234f99c93aec751e9492f2664757d6c029d` |

Only `tbprobe.c` is compiled (it `#include`s `tbchess.c` internally).

## ABI notes

Fathom's public `tb_probe_wdl` and `tb_probe_root` are `static inline`
functions in `tbprobe.h` (they reject nonzero castling rights, and for WDL a
nonzero 50-move counter, then call the exported `tb_probe_wdl_impl` /
`tb_probe_root_impl`). Rust cannot call `static inline` C, so `src/lib.rs`
declares the `*_impl` functions plus `tb_init`, `tb_free`, and the
`TB_LARGEST` global, and replicates the inline wrapper logic in safe Rust.
The declarations and the `TB_RESULT_*` bit layout match the commit above; if
the vendored Fathom is ever updated, re-verify `tbprobe.h` against the `ffi`
module.

Threading contract (from upstream docs): `tb_init`/`tb_free` are not
thread-safe (guarded here by a process-global mutex — at most one `Tablebase`
per process); WDL probes are thread-safe (`&self`); root probes are not
(`&mut self`).

## Usage

```rust,no_run
use silman_tb::{Tablebase, Wdl};

let tb = Tablebase::init(std::path::Path::new("/path/to/syzygy"))?;
let board: cozy_chess::Board = "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1".parse()?;
assert_eq!(tb.probe_board(&board)?, Wdl::Win);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Constraints on probed positions: no castling rights, piece count ≤
`Tablebase::largest()`, and (WDL only) 50-move counter of zero. Bare-kings
positions short-circuit to `Wdl::Draw` (Syzygy sets have no 2-man table).

## Tests

`tests/probe.rs` probes real tablebase files from `testdata/syzygy/` at the
repo root (git-ignored). Fetch them (~26 KB, complete 3-man set from the
Lichess mirror) with:

```sh
bash scripts/fetch_syzygy_test_files.sh
```

Without the files the tests skip gracefully (note on stderr) so CI stays
green. Every FEN's expected value is cross-checked against the Lichess
tablebase API; citations are in the test comments.
