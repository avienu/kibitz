#!/usr/bin/env bash
# Submit a signed Kibitz artifact (.dmg, or a .zip of the .app) to Apple
# notarization and wait for the verdict.
#
# Usage:
#   scripts/release/notarize_mac.sh [--dry-run] [path/to/Kibitz.dmg]
#
# Environment:
#   APPLE_ID            Apple account email used for notarization.
#   APPLE_TEAM_ID       10-character team identifier.
#   APPLE_APP_PASSWORD  App-specific password (appleid.apple.com → App-Specific Passwords).
#
# Default artifact: the newest .dmg under
#   app/src-tauri/target/release/bundle/dmg/
#
# --dry-run validates tooling + inputs and exits 0 without submitting, even
# when secrets are missing. Without --dry-run, missing inputs exit 1.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DMG_DIR="$REPO_ROOT/app/src-tauri/target/release/bundle/dmg"

DRY_RUN=0
ARTIFACT=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) ARTIFACT="$arg" ;;
  esac
done
if [[ -z "$ARTIFACT" && -d "$DMG_DIR" ]]; then
  ARTIFACT="$(ls -t "$DMG_DIR"/*.dmg 2>/dev/null | head -1 || true)"
fi

fail_or_skip() {
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "notarize_mac [dry-run] SKIP: $1"
    exit 0
  fi
  echo "notarize_mac ERROR: $1" >&2
  exit 1
}

command -v xcrun >/dev/null 2>&1 || fail_or_skip "xcrun not found (need Xcode command line tools)"
xcrun notarytool --version >/dev/null 2>&1 || fail_or_skip "xcrun notarytool unavailable (need Xcode 13+ command line tools)"

missing=()
[[ -n "${APPLE_ID:-}" ]] || missing+=(APPLE_ID)
[[ -n "${APPLE_TEAM_ID:-}" ]] || missing+=(APPLE_TEAM_ID)
[[ -n "${APPLE_APP_PASSWORD:-}" ]] || missing+=(APPLE_APP_PASSWORD)
if (( ${#missing[@]} > 0 )); then
  fail_or_skip "missing env: ${missing[*]}. No Apple Developer enrollment yet? Expected — see docs/RELEASE_CHECKLIST.md."
fi

[[ -n "$ARTIFACT" && -f "$ARTIFACT" ]] || fail_or_skip "no artifact found (looked in $DMG_DIR; pass a path explicitly)"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "notarize_mac [dry-run] OK: would submit $ARTIFACT for team $APPLE_TEAM_ID as $APPLE_ID and wait."
  exit 0
fi

echo "notarize_mac: submitting $ARTIFACT (waits for Apple's verdict)"
xcrun notarytool submit "$ARTIFACT" \
  --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_PASSWORD" \
  --wait
echo "notarize_mac: OK — now run scripts/release/staple_mac.sh"
