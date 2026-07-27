#!/usr/bin/env bash
# Fetch the Syzygy tablebase files needed by the kibitz-tb test suite into
# testdata/syzygy/ (git-ignored — Syzygy files are generated data, freely
# redistributed by lichess.org, but they do not belong in the repo).
#
# Downloads the complete 3-man set, WDL (.rtbw) + DTZ (.rtbz), from the
# Lichess mirror. Total size is under 100 KB.
#
# Source: https://tablebase.lichess.ovh/tables/standard/
# (WDL files live under 3-4-5-wdl/, DTZ files under 3-4-5-dtz/)
set -euo pipefail
cd "$(dirname "$0")/.."

BASE_URL="https://tablebase.lichess.ovh/tables/standard"
DEST="testdata/syzygy"
mkdir -p "$DEST"

MATERIAL=(KPvK KNvK KBvK KRvK KQvK)

fetch() {
    local f="$1" url="$2"
    if [[ -s "$DEST/$f" ]]; then
        echo "already present: $DEST/$f"
        return
    fi
    echo "fetching $f"
    curl -fsSL -o "$DEST/$f" "$url"
}

for m in "${MATERIAL[@]}"; do
    fetch "$m.rtbw" "$BASE_URL/3-4-5-wdl/$m.rtbw"
    fetch "$m.rtbz" "$BASE_URL/3-4-5-dtz/$m.rtbz"
done

echo "done:"
ls -la "$DEST"
