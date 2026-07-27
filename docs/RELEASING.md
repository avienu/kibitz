# Releasing Kibitz

How a Kibitz release is built, signed, and published. The one-time setup
items (Apple enrollment, updater keypair, secrets) live in
[RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md); this file is the repeatable
flow.

Current state (pre-1.0): **no Apple Developer enrollment yet** — every
signing/notarization step below is scripted and CI-gated on secrets, and
skips cleanly (with an explicit message) until those secrets exist.
Releases cut before then carry clearly-marked unsigned artifacts and no
updater feed.

## Versions

`0.1.0` is set in four places; keep them in lockstep:

| File | Field |
|---|---|
| `Cargo.toml` (repo root) | `workspace.package.version` (all BSD crates + kibitz-db inherit) |
| `app/src-tauri/Cargo.toml` | `package.version` (standalone GPL package) |
| `app/src-tauri/tauri.conf.json` | `version` (what the bundles and the updater report) |
| `app/package.json` | `version` (run `npm install --package-lock-only` after bumping so `npm ci` stays green) |

## Identity

The bundle identifier is **`org.kibitzchess.app`** (final — the
kibitzchess.org domain is secured; set only in
`app/src-tauri/tauri.conf.json`). Signing, notarization, and updater
artifacts must all carry this identifier — verification is a release
checklist item. Project contact: `contact@kibitzchess.org`.

## Bundle targets

| Platform | Targets | Config |
|---|---|---|
| macOS | `.app`, `.dmg` | `app/src-tauri/tauri.conf.json` → `bundle.targets` |
| Linux | AppImage, `.deb` | `app/src-tauri/tauri.linux.conf.json` (platform-merged override) |

The Linux targets live in a platform-specific config file so each OS
builds exactly its own list — no reliance on the bundler filtering out
foreign targets.

### macOS: arm64-first (deliberate)

Release bundles are built `aarch64-apple-darwin` only, on Apple Silicon
(local M1 Max, `macos-latest` runners in CI). Universal binaries are
**not** produced yet because a universal build needs the `x86_64`
toolchain target plus an Intel machine (or Rosetta) to actually verify
the slice we'd be shipping — an unverified slice is worse than an honest
"Apple Silicon only" label. Intel Mac users build from source (the
`from-source` CI job keeps those instructions honest). Revisit at 1.0;
the updater feed already understands a `darwin-universal` key when we
get there.

### Icon

`app/src-tauri/icons/` holds a custom placeholder set (2×2 board squares
with a knight-path squiggle), not the stock Tauri logo. **Pre-1.0 art
TODO:** commission/design a real icon, regenerate the set with
`npm run tauri icon -- path/to/icon.png`, and refresh the store logos.

## Local build (this is what CI runs too)

```sh
cd app
npm ci
CI=true npm run tauri build
```

`CI=true` matters for local macOS builds: the DMG step runs a
Finder-prettifying AppleScript that needs a TCC automation grant
("Not authorized to send Apple events to Finder (-1743)" from a plain
terminal); `CI=true` makes the bundler pass `--skip-jenkins`, skipping
the cosmetic step. (Alternatively grant your terminal Automation → Finder
in System Settings and drop the variable.)

Output lands under `app/src-tauri/target/release/bundle/`:

- `macos/Kibitz.app`
- `dmg/Kibitz_<version>_aarch64.dmg`
- (Linux) `appimage/*.AppImage`, `deb/*.deb`

## macOS signing, notarization, stapling

Three scripts under `scripts/release/`, each parameterized by env and
each supporting `--dry-run` (validates tooling + inputs, **exit 0**
without touching anything — missing secrets are an explicit SKIP message;
without `--dry-run`, missing inputs exit 1):

| Script | Env | What it does |
|---|---|---|
| `sign_mac.sh [app-path]` | `APPLE_CERT_IDENTITY` (e.g. `Developer ID Application: Name (TEAMID)`) | `codesign` the `.app`, hardened runtime + timestamp, then verify |
| `notarize_mac.sh [dmg-path]` | `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | `xcrun notarytool submit --wait` |
| `staple_mac.sh [dmg-path]` | (none) | `xcrun stapler staple` + Gatekeeper `spctl` verify |

Manual flow, once enrollment exists:

```sh
CI=true npm run tauri build                      # from app/
scripts/release/sign_mac.sh                      # sign the .app
CI=true npx tauri build --bundles dmg            # re-pack the signed app into a dmg (from app/)
scripts/release/notarize_mac.sh                  # submit + wait
scripts/release/staple_mac.sh                    # staple + verify
```

In CI the same signing happens inside `npm run tauri build` itself: the
Tauri bundler picks up `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`
from the environment and performs codesign + notarization during
bundling. The scripts are the local/recovery path and the dry-run
validation surface.

## Updater

Kibitz uses the Tauri v2 updater plugin (`tauri-plugin-updater`):

- **Config**: `app/src-tauri/tauri.conf.json` → `plugins.updater`.
  Endpoint: the `latest.json` asset on the newest GitHub release of
  `avienu/kibitz`. The `pubkey` field currently holds a
  `TODO-UPDATER-PUBKEY` placeholder — checklist item; while it does, the
  in-app updater reports "not configured" and performs **no network
  call**.
- **In-app behavior**: Settings → Updates has "Check for updates"
  (default ON — one check at launch, nothing polls) and a manual "Check
  now". Backed by the `update_check` command
  (`app/src-tauri/src/updates.rs`); the frontend logic and its
  latest.json contract test live in `app/src/lib/updates.ts` /
  `updates.test.ts` (mock fixture, no network).

### Key generation (one-time; checklist item 3)

```sh
cd app
npm run tauri signer generate -- -w ~/.tauri/kibitz-updater.key
```

Storage expectations:

- The **private key + its password** go in the password manager (and only
  there). **NEVER commit it**, never store it in the repo directory, and
  don't leave the unencrypted key on disk. CI gets it as the
  `TAURI_SIGNING_PRIVATE_KEY` (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`)
  secrets — paste the key file's contents.
- The **public key** replaces the `TODO-UPDATER-PUBKEY` placeholder in
  `tauri.conf.json` and is committed.
- Losing the private key strands every installed copy on its version:
  treat it like a signing cert, with a backup in a second vault.

### Feed

`scripts/release/generate_latest_json.mjs` builds `latest.json` from the
collected updater artifacts (`*.app.tar.gz` + `.sig` → `darwin-<arch>`,
`*.AppImage` + `.sig` → `linux-<arch>`) with GitHub release download
URLs. CI runs it only when the signing key exists (unsigned artifacts
must never appear in a feed) and uploads it as a release asset — the
endpoint above always points at the latest release's copy.

Updater artifacts themselves are produced by building with
`--config '{"bundle":{"createUpdaterArtifacts":true}}'`; the flag stays
out of the checked-in config because enabling it makes builds **fail**
when `TAURI_SIGNING_PRIVATE_KEY` is absent (i.e. every local/pre-key
build).

## CI release workflow (`.github/workflows/release.yml`)

- **Trigger**: tag `v*` → full run; `workflow_dispatch` → dry-run
  (bundles + workflow artifacts, no release). Main `ci.yml` is
  deliberately untouched — no bundle job there, it stays fast.
- **`bundle` matrix**: macOS aarch64 + Linux x86_64. Signing steps are
  conditional on secrets and skip with a note in the job summary when
  absent.
- **`release` job** (tag pushes only): downloads all bundles, generates
  `latest.json` when the updater key exists, and creates a **draft**
  GitHub release — with signed artifacts + feed when secrets exist,
  otherwise with the unsigned artifacts and a notes stub that says
  UNSIGNED in bold. Publishing the draft is always a human act
  (checklist item 4).
- **`from-source` job**: ubuntu + macOS, executes the README's literal
  fresh-clone commands (`rustup default stable`, `npm ci` in `app/`,
  `cargo test --workspace`, `npm run tauri build`) so the published
  build-from-source story can't rot silently.

### Secrets (exact names)

| Secret | Used for |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | updater artifact signing (contents of the generated key file) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password for the above |
| `APPLE_ID` | notarization (Apple account email) |
| `APPLE_TEAM_ID` | notarization / signing team |
| `APPLE_APP_PASSWORD` | app-specific password for notarytool |
| `APPLE_CERT_IDENTITY` | local scripts: codesign identity string |
| `APPLE_CERTIFICATE` | CI only: base64 of the Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | CI only: password of that `.p12` |
| `APPLE_SIGNING_IDENTITY` | CI only: identity string for the bundler (same value as `APPLE_CERT_IDENTITY`) |

## Cutting a release, end to end

1. Bump versions (table above), update `CHANGELOG.md`, commit.
2. `git tag v<version> && git push origin v<version>`.
3. Wait for the Release workflow; check the run summary for skipped-signing notes.
4. Open the draft release, replace the notes stub with the CHANGELOG
   section, publish.
5. If signed: verify the updater against the published feed from an older
   installed build (checklist item 5).
