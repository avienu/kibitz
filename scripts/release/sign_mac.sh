#!/usr/bin/env bash
# Codesign the Kibitz .app bundle with a Developer ID Application identity.
#
# Usage:
#   scripts/release/sign_mac.sh [--dry-run] [path/to/Kibitz.app]
#
# Environment:
#   APPLE_CERT_IDENTITY  Signing identity, e.g. 'Developer ID Application: Jane Doe (TEAMID1234)'.
#                        Must exist in the login keychain (see docs/RELEASING.md).
#
# Default app path: the local tauri build output
#   app/src-tauri/target/release/bundle/macos/Kibitz.app
#
# --dry-run validates tooling + inputs and exits 0 without signing, even when
# secrets are missing (this is the CI-gating mode). Without --dry-run, missing
# inputs exit 1.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEFAULT_APP="$REPO_ROOT/app/src-tauri/target/release/bundle/macos/Kibitz.app"

DRY_RUN=0
APP_PATH=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) APP_PATH="$arg" ;;
  esac
done
APP_PATH="${APP_PATH:-$DEFAULT_APP}"

fail_or_skip() {
  # $1: message
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "sign_mac [dry-run] SKIP: $1"
    exit 0
  fi
  echo "sign_mac ERROR: $1" >&2
  exit 1
}

command -v codesign >/dev/null 2>&1 || fail_or_skip "codesign not found (need Xcode command line tools: xcode-select --install)"

[[ -n "${APPLE_CERT_IDENTITY:-}" ]] || fail_or_skip "APPLE_CERT_IDENTITY is not set (e.g. 'Developer ID Application: Name (TEAMID)'). No Apple Developer enrollment yet? This is expected — see docs/RELEASE_CHECKLIST.md."

if ! security find-identity -v -p codesigning 2>/dev/null | grep -Fq "$APPLE_CERT_IDENTITY"; then
  fail_or_skip "identity '$APPLE_CERT_IDENTITY' not found in the keychain (security find-identity -v -p codesigning)"
fi

[[ -d "$APP_PATH" ]] || fail_or_skip "app bundle not found at $APP_PATH (run: cd app && npm run tauri build)"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "sign_mac [dry-run] OK: would sign $APP_PATH as '$APPLE_CERT_IDENTITY' (hardened runtime, timestamped)."
  exit 0
fi

echo "sign_mac: signing $APP_PATH"
codesign --force --deep --options runtime --timestamp \
  --sign "$APPLE_CERT_IDENTITY" "$APP_PATH"

echo "sign_mac: verifying"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
echo "sign_mac: OK"
