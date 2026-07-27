# Release checklist (path to a signed, auto-updating 0.1.0)

One-time setup, in order. The repeatable release flow lives in
[RELEASING.md](RELEASING.md). Items 2–3 are the only ones that cost
money or create secrets; everything before and after is already
scripted and verified. CI skips signing cleanly until the secrets in
items 2–3 exist, so releases can be cut (clearly marked unsigned) at any
point along this list.

## 1. Identity (bundle ID final; mail pending)

- [x] Bundle identifier finalized: **`org.kibitzchess.app`**
      (kibitzchess.org secured; set in `app/src-tauri/tauri.conf.json`
      only — commit 350350e).
- [ ] Set up `contact@kibitzchess.org` mail forwarding **before release**
      (registrar/DNS forwarding to the maintainer inbox; this address is
      the UA contact default and will receive notarization and user
      mail).
- [ ] After the first signed build: verify `org.kibitzchess.app` appears
      consistently across signing/notarization artifacts —
      `codesign -dv Kibitz.app` (Identifier), the notarytool submission
      log, and the stapled ticket (`xcrun stapler validate`).

## 2. Apple Developer enrollment + certificate (exact steps)

- [ ] Enroll the maintainer Apple ID in the Apple Developer Program
      (developer.apple.com/enroll, $99/yr; sole proprietor is fine).
      Note the 10-character **Team ID**.
- [ ] Create a **Developer ID Application** certificate:
      Xcode → Settings → Accounts → Manage Certificates → “+” →
      Developer ID Application (or developer.apple.com/account →
      Certificates → Create → Developer ID Application with a local CSR).
      It lands in the login keychain; export a `.p12` with a strong
      password into the password manager.
- [ ] Create an **app-specific password** for notarization:
      account.apple.com → Sign-In and Security → App-Specific Passwords.
      Store it in the password manager.
- [ ] Set the four core secrets (GitHub → repo → Settings → Secrets →
      Actions; same names in the local shell for the
      `scripts/release/*.sh` scripts):
      1. `APPLE_ID` — Apple account email
      2. `APPLE_TEAM_ID` — the Team ID from enrollment
      3. `APPLE_APP_PASSWORD` — the app-specific password
      4. `APPLE_CERT_IDENTITY` — the identity string, e.g.
         `Developer ID Application: <Name> (<TEAMID>)`
- [ ] For CI-side codesigning additionally set:
      `APPLE_CERTIFICATE` (base64 of the exported `.p12`:
      `base64 -i cert.p12 | pbcopy`), `APPLE_CERTIFICATE_PASSWORD`, and
      `APPLE_SIGNING_IDENTITY` (same value as `APPLE_CERT_IDENTITY`).
- [ ] Verify: `scripts/release/sign_mac.sh --dry-run` and
      `notarize_mac.sh --dry-run` now report OK instead of SKIP.

## 3. Updater keypair

- [x] Generate: `cd app && npm run tauri signer generate -- -w ~/.tauri/kibitz-updater.key` — DONE 2026-07-27 (pubkey committed, secrets set, private key vaulted)
      (choose a password).
- [ ] Store private key + password in the password manager (plus a
      second-vault backup). **NEVER committed**; delete stray plaintext
      copies. Losing it strands every install on its version.
- [ ] Paste the public key over the `TODO-UPDATER-PUBKEY` placeholder in
      `app/src-tauri/tauri.conf.json` → `plugins.updater.pubkey`; commit.
      (The Settings → Updates row stops saying "not configured" from the
      next build.)
- [ ] Set CI secrets `TAURI_SIGNING_PRIVATE_KEY` (key file contents) and
      `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## 4. Cut v0.1.0

- [ ] Versions already at 0.1.0 (root workspace, `app/src-tauri`,
      `tauri.conf.json`, `app/package.json`) — re-verify before tagging.
- [ ] `git tag v0.1.0 && git push origin v0.1.0`; wait for the Release
      workflow (check run summaries for skipped-signing notes — with
      items 2–3 done there should be none).
- [ ] On the draft release: replace the notes stub with the 0.1.0
      section of `CHANGELOG.md`; confirm artifacts (.dmg, .app.tar.gz +
      .sig, .AppImage + .sig, .deb, latest.json); publish.

## 5. Verify the updater against the published feed

- [ ] Install the previous build (or a locally-built lower version),
      Settings → Updates → Check now: it must offer v0.1.0 from the
      published `latest.json`.
- [ ] Confirm the GitHub `releases/latest/download/latest.json` URL
      resolves and its `platforms` keys cover darwin-aarch64 and
      linux-x86_64.

## 6. Screenshot refresh cadence

- [ ] Refresh `docs/screenshots/` at every tagged release (they are the
      README/website storefront): retake the standard set against the
      release build, same games/positions, both themes where shown.
      Stale screenshots are worse than none — if a screen changed and
      wasn't retaken, drop it from the set rather than ship the old one.

## 7. Go-public switch order (strict)

1. [ ] Audit sign-off (licensing/cleanroom/security audit complete).
2. [ ] Repo public.
3. [ ] Release published (never publish a release from the
       still-private repo; the updater endpoint and from-source docs
       must be publicly resolvable the moment a release exists).
