# Third-Party Notices

Kibitz is dual-layered: the `crates/*` workspace members are licensed
BSD-3-Clause (see `LICENSE-BSD`), and the `app/` layer (Tauri shell,
`app/kibitz-db`, front end) is licensed GPL-3.0 (see `LICENSE-GPL`).
This file lists the third-party software, fonts, artwork, and datasets that
Kibitz depends on or bundles, grouped by license.

Generated 2026-07-26 from `cargo license` (root workspace and
`app/src-tauri`), `license-checker` (npm), and the bundled LICENSE files.
Versions are the currently locked versions; see `Cargo.lock`,
`app/src-tauri/Cargo.lock`, and `app/package-lock.json` for the exact pins.

---

## 1. Rust dependencies (production)

### Direct dependencies

| Crate | Version | License | Layer |
|---|---|---|---|
| cozy-chess (+ cozy-chess-types) | 0.3.4 (0.2.2) | MIT | BSD crates, app layer, bench |
| serde | 1.x | MIT OR Apache-2.0 | BSD crates, app layer |
| serde_json | 1.x | MIT OR Apache-2.0 | BSD crates, app layer |
| thiserror | 2.x | MIT OR Apache-2.0 | BSD crates |
| cc | 1.x | MIT OR Apache-2.0 | build-dep of kibitz-tb (compiles vendored Fathom) |
| anyhow | 1.x | MIT OR Apache-2.0 | GPL app layer |
| clap | 4.x | MIT OR Apache-2.0 | GPL app layer (CLI) |
| rusqlite (bundled SQLite) | 0.32 | MIT (SQLite itself is public domain) | GPL app layer |
| ureq | 2.x | MIT OR Apache-2.0 | GPL app layer (ingesters) |
| zip | 2.x | MIT | GPL app layer (TWIC archives) |
| tauri | 2.11.5 | Apache-2.0 OR MIT | GPL app layer (app/src-tauri) |
| tauri-build | 2.6.3 | Apache-2.0 OR MIT | build-dep (app/src-tauri) |
| tokio | 1.53.1 | MIT | GPL app layer (UCI subprocess I/O) |

### Transitive dependencies, grouped by license

Root cargo workspace (`crates/*`, `app/kibitz-db`, `bench/`). This list is
from `cargo license` over the whole workspace and therefore also contains
dev/bench-only crates (the criterion, insta, tempfile, and shakmaty trees);
those are marked in section 3.

- **Apache-2.0 OR MIT** (119 crates): ahash, anes, anstream, anstyle, anstyle-parse, anstyle-query, anstyle-wincon, anyhow, arbitrary, arrayvec, autocfg, base64, bitflags, btoi, bumpalo, cast, cc, cfg-if, clap, clap_builder, clap_derive, clap_lex, colorchoice, crc32fast, criterion, criterion-plot, crossbeam-deque, crossbeam-epoch, crossbeam-utils, derive_arbitrary, displaydoc, either, encode_unicode, equivalent, errno, fallible-iterator, fallible-streaming-iterator, fastrand, find-msvc-tools, flate2, form_urlencoded, futures-core, futures-task, futures-util, getrandom, half, hashbrown, hashlink, heck, hermit-abi, idna, idna_adapter, indexmap, is_terminal_polyfill, itertools, itoa, js-sys, libc, log, nohash-hasher, num-traits, once_cell, once_cell_polyfill, percent-encoding, pest, pest_derive, pest_generator, pest_meta, pin-project-lite, pkg-config, proc-macro2, quote, rayon, rayon-core, regex, regex-automata, regex-syntax, rustls-pki-types, rustversion, serde, serde_core, serde_derive, serde_json, shlex, smallvec, stable_deref_trait, syn, tempfile, thiserror, thiserror-impl, tinytemplate, ucd-trie, ureq, url, utf8_iter, utf8parse, vcpkg, version_check, wasm-bindgen (+macro/support/shared), web-sys, windows-link, windows-sys, windows-targets, windows_* target crates, zeroize
- **MIT** (17 crates): console, cozy-chess, cozy-chess-types, crunchy, is-terminal, libsqlite3-sys, oorandom, plotters, plotters-backend, plotters-svg, rusqlite, simd-adler32, slab, strsim, synstructure, zip, zmij
- **Apache-2.0** (6 crates): ciborium, ciborium-io, ciborium-ll, insta, similar, zopfli
- **Unicode-3.0** (18 crates, ICU4X): icu_collections, icu_locale_core, icu_normalizer, icu_normalizer_data, icu_properties, icu_properties_data, icu_provider, litemap, potential_utf, tinystr, writeable, yoke, yoke-derive, zerofrom, zerofrom-derive, zerotrie, zerovec, zerovec-derive
- **MIT OR Unlicense** (5 crates): aho-corasick, memchr, same-file, walkdir, winapi-util
- **BSD-3-Clause**: subtle
- **ISC**: rustls-webpki, untrusted
- **Apache-2.0 AND ISC**: ring
- **Apache-2.0 OR ISC OR MIT**: rustls
- **CDLA-Permissive-2.0**: webpki-roots
- **(Apache-2.0 OR MIT) AND Unicode-3.0**: unicode-ident
- **0BSD OR Apache-2.0 OR MIT**: adler2
- **Apache-2.0 OR MIT OR Zlib**: miniz_oxide
- **Apache-2.0 OR BSD-2-Clause OR MIT**: zerocopy, zerocopy-derive
- **Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT**: linux-raw-sys, rustix, wasi
- **Apache-2.0 OR LGPL-2.1-or-later OR MIT**: r-efi (used under Apache-2.0/MIT; LGPL is one option of a tri-license)
- **GPL-3.0-or-later**: shakmaty — **bench-only** comparison baseline in `bench/movegen-bench`; never shipped and never a dependency of `crates/*`. See section 3.

`app/src-tauri` (standalone GPL-3.0 package with its own lockfile) additionally
pulls the Tauri/wry/tao ecosystem. Grouped by license (crate names in
`app/src-tauri/Cargo.lock`; run `cargo license` there for the full
per-crate list):

- **Apache-2.0 OR MIT** (278 crates): the tauri family (tauri, tauri-build, tauri-codegen, tauri-macros, tauri-runtime, tauri-runtime-wry, tauri-utils), wry, chrono, reqwest, serde family, url, uuid, windows-* crates, and others
- **MIT** (105 crates): tokio (+tokio-macros, tokio-util), tracing, tracing-core, hyper, hyper-util, tower family, gtk/glib/webkit2gtk bindings (Linux), objc2 core crates, rusqlite, libsqlite3-sys, cozy-chess, zip, phf family, and others
- **Apache-2.0 OR MIT OR Zlib** (20 crates): objc2-* framework bindings (macOS/iOS), raw-window-handle, tinyvec, bytemuck, dispatch2, miniz_oxide
- **Unicode-3.0** (18 crates): ICU4X (icu_*, zerovec, yoke, …)
- **MPL-2.0** (5 crates): cssparser, cssparser-macros, dtoa-short, option-ext, selectors — weak file-level copyleft, unmodified, source available upstream; GPL-compatible, app layer only
- **Apache-2.0** (3): sync_wrapper, tao, zopfli
- **ISC** (3): libloading, rustls-webpki, untrusted
- **BSD-3-Clause** (2 third-party): alloc-no-stdlib, alloc-stdlib; **BSD-3-Clause AND MIT**: brotli; **BSD-3-Clause OR MIT**: brotli-decompressor
- **Zlib**: foldhash
- **CDLA-Permissive-2.0**: webpki-roots
- **Apache-2.0 AND ISC**: ring; **Apache-2.0 AND MIT**: dpi; **Apache-2.0 WITH LLVM-exception**: target-lexicon
- **MIT OR Unlicense** (6): aho-corasick, byteorder, memchr, same-file, walkdir, winapi-util
- Various permissive multi-licensed crates: adler2 (0BSD/Apache-2.0/MIT), dunce (Apache-2.0/CC0-1.0/MIT-0), num_enum (Apache-2.0/BSD-3-Clause/MIT), r-efi (Apache-2.0/LGPL-2.1-or-later/MIT — used under Apache-2.0/MIT), rustls (Apache-2.0/ISC/MIT), unicode-ident ((Apache-2.0 OR MIT) AND Unicode-3.0), wasi/wasip2/wit-bindgen/rustix/linux-raw-sys (Apache-2.0/LLVM-exception/MIT), zerocopy (Apache-2.0/BSD-2-Clause/MIT)

No GPL, LGPL, or AGPL third-party code is present in the dependency tree of
any BSD-3-Clause crate (`crates/*`). Verified with
`cargo tree -e normal,build` per crate on 2026-07-26.

## 2. npm dependencies (production)

From `license-checker --production` in `app/` (includes transitive):

| Package | Version | License |
|---|---|---|
| chessground | 9.2.1 | **GPL-3.0-or-later** |
| chessops | 0.14.2 | **GPL-3.0-or-later** |
| @badrap/result (via chessops) | 0.2.13 | MIT |
| @tauri-apps/api | 2.11.1 | Apache-2.0 OR MIT |
| react | 19.2.8 | MIT |
| react-dom | 19.2.8 | MIT |
| scheduler (via react-dom) | 0.27.0 | MIT |

chessground and chessops are GPL-3.0-or-later and are used only in the
GPL-3.0 `app/` layer, in accordance with the project's licensing boundary.

## 3. Build- and development-only dependencies (not shipped)

These are used for testing, benchmarking, and building only. They are not
distributed with Kibitz.

### Rust dev-dependencies

| Crate | Version | License | Used for |
|---|---|---|---|
| criterion (+criterion-plot) | 0.5.1 | MIT OR Apache-2.0 | benchmarks |
| insta | 1.48.0 | Apache-2.0 | snapshot tests |
| tempfile | 3.x | MIT OR Apache-2.0 | tests |
| shakmaty | 0.27 | **GPL-3.0-or-later** | Phase 0 movegen benchmark baseline in `bench/movegen-bench` only. Never linked into `crates/*` or any shipped artifact. |

(Their transitive dependencies — plotters, rayon, ciborium, console,
similar, etc. — appear in the grouped list in section 1 and are likewise
dev/bench-only.)

### npm devDependencies

Direct devDependencies (from `app/package.json` / installed versions):

| Package | Version | License |
|---|---|---|
| @tauri-apps/cli | 2.11.4 | Apache-2.0 OR MIT |
| @testing-library/dom | 10.4.1 | MIT |
| @testing-library/react | 16.3.2 | MIT |
| @types/react | 19.2.17 | MIT |
| @types/react-dom | 19.2.3 | MIT |
| @vitejs/plugin-react | 4.7.0 | MIT |
| jsdom | 29.1.1 | MIT |
| typescript | 5.9.3 | Apache-2.0 |
| vite | 6.4.3 | MIT |
| vitest | 3.2.7 | MIT |

Full dev-install license breakdown (153 packages, `license-checker
--development`): 129 MIT, 7 ISC, 5 Apache-2.0, 2 Apache-2.0 OR MIT,
2 BSD-2-Clause, 2 BSD-3-Clause, 2 MIT-0 (@csstools/color-helpers,
@csstools/css-syntax-patches-for-csstree), 1 BlueOak-1.0.0 (lru-cache),
1 CC-BY-4.0 (caniuse-lite browser-support data), 1 CC0-1.0 (mdn-data).

## 4. Vendored code

### Fathom (Syzygy tablebase probing) — MIT

Vendored verbatim at `crates/kibitz-tb/vendor/fathom/` from
<https://github.com/jdart1/Fathom> and compiled/statically linked into
`kibitz-tb` (whose SPDX expression is therefore `BSD-3-Clause AND MIT`).
Its LICENSE file (The MIT License) is preserved in the vendor directory:

> Copyright (c) 2013-2018 Ronald de Man
> Copyright (c) 2015 basil00
> Copyright (c) 2016-2025 by Jon Dart

## 5. Fonts — SIL Open Font License 1.1

The following fonts are bundled as woff2 subsets in `app/public/fonts/`,
each with its full OFL-1.1 license text alongside the font files:

- **Public Sans** (v21, latin) — a Modified Version (by the U.S. General
  Services Administration) of Libre Franklin. License:
  `app/public/fonts/public-sans/LICENSE.md`.
- **Source Serif 4** (v14, latin) — Copyright 2014–2023 Adobe, with
  Reserved Font Name 'Source'. License:
  `app/public/fonts/source-serif-4/LICENSE.md`.
- **JetBrains Mono** (v24, latin) — Copyright 2020 The JetBrains Mono
  Project Authors. License: `app/public/fonts/jetbrains-mono/OFL.txt`.

## 6. Datasets and artwork

- **Lichess openings dataset** (lichess-org/chess-openings) — **CC0-1.0**.
  Bundled at `data/openings/*.tsv` (ECO classification).
- **Lichess puzzle database** — **CC0-1.0**
  (<https://database.lichess.org/#puzzles>). A 500-row test fixture is
  committed at `testdata/fixtures/puzzles_sample.csv`; the full dump is
  downloaded by the user via `kibitz-cli import-puzzles` and is not
  distributed with Kibitz. Provenance is recorded in the database
  `sources` table on import.
- **Chess piece artwork (cburnett set)** by Colin M. L. Burnett — shipped
  as embedded SVG data URIs inside chessground's
  `assets/chessground.cburnett.css`, i.e. distributed here as part of
  chessground under **GPL-3.0-or-later** (GPL app layer only). TODO: the
  upstream piece set is multi-licensed on Wikimedia Commons
  (CC-BY-SA / GPL); verify the upstream terms before reusing the SVGs
  standalone outside chessground.
- **TWIC (The Week in Chess)** — not bundled and never redistributed; the
  ingester downloads issues to the user's machine for personal use only.

## 7. Stockfish

Stockfish (**GPL-3.0**) is not part of this repository and is not linked
into any Kibitz binary. Kibitz communicates with a user-provided Stockfish
executable at arm's length as a separate UCI subprocess. Users obtain
Stockfish and its source from <https://stockfishchess.org/>.

## 8. Syzygy tablebases

Tablebase files (`testdata/syzygy/`, user-downloaded) are probed via the
vendored Fathom code (section 4). The tablebase data files themselves are
not distributed with Kibitz.
