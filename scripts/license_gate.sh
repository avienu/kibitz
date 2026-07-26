#!/usr/bin/env bash
# License gate: fail if any GPL/LGPL/AGPL-licensed crate appears anywhere in
# the dependency tree of a BSD-3-Clause workspace member (crates/*).
# Uses `cargo tree`'s license formatting so no extra tooling is required.
set -euo pipefail
cd "$(dirname "$0")/.."

FORBIDDEN='GPL|LGPL|AGPL'
BSD_CRATES=(silman-core silman-profile silman-srs silman-verbalize silman-tb si4-read)
status=0

for pkg in "${BSD_CRATES[@]}"; do
    out=$(cargo tree -p "$pkg" -e normal,build,dev --prefix none --format "{p} | {l}" | sort -u)
    if bad=$(grep -E "$FORBIDDEN" <<<"$out"); then
        echo "License gate FAILED for $pkg — forbidden licenses in dependency tree:"
        echo "$bad"
        status=1
    else
        count=$(wc -l <<<"$out" | tr -d ' ')
        echo "License gate OK for $pkg ($count packages checked)"
    fi
done

exit "$status"
