#!/usr/bin/env bash
# Staple the notarization ticket to a notarized Kibitz artifact and verify
# the result with Gatekeeper.
#
# Usage:
#   scripts/release/staple_mac.sh [--dry-run] [path/to/Kibitz.dmg-or-.app]
#
# No secrets required — stapling needs only a previously notarized artifact
# and network access to Apple's ticket service.
#
# Default artifact: the newest .dmg under
#   app/src-tauri/target/release/bundle/dmg/
#
# --dry-run validates tooling + inputs and exits 0 without stapling.
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
    echo "staple_mac [dry-run] SKIP: $1"
    exit 0
  fi
  echo "staple_mac ERROR: $1" >&2
  exit 1
}

command -v xcrun >/dev/null 2>&1 || fail_or_skip "xcrun not found (need Xcode command line tools)"
xcrun --find stapler >/dev/null 2>&1 || fail_or_skip "xcrun stapler unavailable (stapler ships with full Xcode, not the bare command line tools)"

[[ -n "$ARTIFACT" && -e "$ARTIFACT" ]] || fail_or_skip "no artifact found (looked in $DMG_DIR; pass a path explicitly)"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "staple_mac [dry-run] OK: would staple + Gatekeeper-verify $ARTIFACT."
  exit 0
fi

echo "staple_mac: stapling $ARTIFACT"
xcrun stapler staple "$ARTIFACT"

echo "staple_mac: Gatekeeper verification"
spctl --assess --type open --context context:primary-signature -v "$ARTIFACT" \
  || spctl --assess --type execute -v "$ARTIFACT"
echo "staple_mac: OK"
