# Release checklist (path to a signed, auto-updating 0.1.0)

One-time setup, in order. The repeatable release flow lives in
[RELEASING.md](RELEASING.md). Items 2–3 are the only ones that cost
money or create secrets; everything before and after is already
scripted and verified. CI skips signing cleanly until the secrets in
items 2–3 exist, so releases can be cut (clearly marked unsigned) at any
point along this list.

## How this list fails you

Every signing failure met so far reported something true about the wrong
layer. None was a wrong message; none pointed at its fix:

| What it said | What was actually wrong |
|---|---|
| `0 valid identities found` | Apple's intermediates missing, so the certificate was untrusted — and an untrusted certificate is not an identity |
| `certificate ... does not match provided identity` | the `.p12` held `Apple Distribution` instead of `Developer ID Application` |
| `Invalid symbol 37, offset 348` | a `%` copied out of a terminal along with the key |

So when a step here fails, do not start from the message's own layer.
Check what it presupposes: that the certificate is trusted, that it is
the right *kind* of certificate, that a secret holds exactly the bytes of
the file it came from. The steps below name those preconditions because
each one has already cost an hour.

## 1. Identity (bundle ID final; mail pending)

- [x] Bundle identifier finalized: **`org.kibitzchess.kibitz`**
      (kibitzchess.org secured; set in `app/src-tauri/tauri.conf.json`
      only — commit 350350e).
- [ ] Set up `contact@kibitzchess.org` mail forwarding **before release**
      (registrar/DNS forwarding to the maintainer inbox; this address is
      the UA contact default and will receive notarization and user
      mail).
- [ ] After the first signed build: verify `org.kibitzchess.kibitz` appears
      consistently across signing/notarization artifacts —
      `codesign -dv Kibitz.app` (Identifier), the notarytool submission
      log, and the stapled ticket (`xcrun stapler validate`).

## 2. Apple Developer enrollment + certificate (exact steps)

- [ ] Enroll the maintainer Apple ID in the Apple Developer Program
      (developer.apple.com/enroll, $99/yr; sole proprietor is fine).
      Note the 10-character **Team ID**.
- [ ] **Install the Apple intermediate certificates FIRST**, from
      https://www.apple.com/certificateauthority/ — the *Worldwide
      Developer Relations* intermediate AND the *Developer ID* one.
      Download, double-click, done.

      Skipping this costs an hour and gives you no way to spend it well.
      A Developer ID certificate that imports perfectly will still leave
      `security find-identity -v -p codesigning` reporting
      `0 valid identities found`, because without the intermediate the
      chain to Apple's root is broken and an untrusted certificate is not
      a valid identity. The message is accurate and names the wrong
      layer: nothing in it mentions trust, the chain, or intermediates,
      and the certificate is plainly right there in Keychain Access.
      (2026-08-01: this is exactly what happened.)
- [ ] Create a **Developer ID Application** certificate:
      Xcode → Settings → Accounts → Manage Certificates → “+” →
      Developer ID Application (or developer.apple.com/account →
      Certificates → Create → Developer ID Application with a local CSR).
      It lands in the login keychain; export a `.p12` with a strong
      password into the password manager.

      **Export the one whose name begins `Developer ID Application:`.**
      If the team has ever touched App Store distribution there will also
      be an `Apple Distribution:` certificate, sitting right beside it in
      Keychain Access, carrying the same organization name and looking
      equally correct. It is for the App Store and ad-hoc builds; a
      direct-download app signed with it is rejected outright. The
      bundler catches the mismatch — `certificate from APPLE_CERTIFICATE
      "Apple Distribution: ..." does not match provided identity` — but
      only after a full release build (2026-08-01: it did).
      `security find-identity -v -p codesigning` lists every identity;
      read the whole list, not the count.
- [ ] **Back the `.p12` up before doing anything else.** The private key
      exists only in the login keychain until it is exported. Lose it and
      the same identity can never sign again — for Developer ID that means
      every existing user gets a fresh Gatekeeper prompt on the next
      update. Password manager plus a second, offline vault.
- [ ] Notarization credential — **App Store Connect API key preferred**
      (revocable on its own, no account password in CI):
      App Store Connect → Users and Access → Integrations → App Store
      Connect API → “+”, Developer access. Apple serves the `.p8` ONCE;
      losing it means issuing a new key.
      - `APPLE_API_ISSUER` — the Issuer ID (a UUID, one per team)
      - `APPLE_API_KEY` — the Key ID (10 characters)
      - `APPLE_API_KEY_P8` — base64 of the `.p8`:
        `base64 -i AuthKey_XXXXXXXXXX.p8 | pbcopy`
      The older route still works if the key ones are absent: `APPLE_ID`,
      `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` (app-specific password from
      account.apple.com → Sign-In and Security).
- [ ] Set the codesigning secrets (GitHub → repo → Settings → Secrets →
      Actions):
      1. `APPLE_CERTIFICATE` — base64 of the exported `.p12`:
         `base64 -i cert.p12 | pbcopy`
      2. `APPLE_CERTIFICATE_PASSWORD` — the `.p12` password
      3. `APPLE_SIGNING_IDENTITY` — the identity string, e.g.
         `Developer ID Application: <Name> (<TEAMID>)`
      `APPLE_CERT_IDENTITY` is the same string, for the local
      `scripts/release/*.sh` scripts.
- [ ] Sanity: the release build FAILS if `APPLE_CERTIFICATE` is set with
      no notarization credential. That is deliberate — Gatekeeper treats a
      signed, un-notarized app worse than an unsigned one.
- [ ] Verify: `scripts/release/sign_mac.sh --dry-run` and
      `notarize_mac.sh --dry-run` now report OK instead of SKIP.

## 3. Updater keypair

- [x] Generate: `cd app && npm run tauri signer generate -- -w ~/.tauri/kibitz-updater.key` — DONE 2026-07-27 (pubkey committed, secrets set, private key vaulted)
      (choose a password).
- [ ] Store private key + password in the password manager (plus a
      second-vault backup). **NEVER committed**; delete stray plaintext
      copies. Losing it strands every install on its version.
- [ ] Set `TAURI_SIGNING_PRIVATE_KEY` **from the file, never from a
      terminal window**: `pbcopy < ~/.tauri/kibitz-updater.key`. Copying
      what the terminal displays picks up zsh's `%` end-of-partial-line
      marker, and the build fails at bundle time with `failed to decode
      base64 secret key: Invalid symbol 37` — symbol 37 being that `%`,
      reported at its offset with no hint of where it came from
      (2026-08-01: it did).
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
