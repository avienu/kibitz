#!/usr/bin/env bash
# Grep-gate (run 8): the working-era name "silman" may appear only in
# design/handoff-*/ (preserved verbatim), historical RUN_REPORT.md
# sections, and author attributions to Jeremy Silman (the person the
# engine's teaching style is named after).
set -euo pipefail
cd "$(dirname "$0")/.."
# testdata/private is git-ignored (never ships) and holds book-trial
# transcriptions whose notes cite the author's own game captions
# ("Silman-Wolski") and book titles verbatim.
hits=$(grep -rIni 'silman' \
  --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules \
  --exclude-dir=dist --exclude-dir=handoff-1 --exclude-dir=handoff-2 \
  --exclude-dir=testdata \
  --exclude-dir=private \
  --exclude=RUN_REPORT.md --exclude=name_gate.sh \
  . | grep -vi 'jeremy silman' || true)
if [[ -n "$hits" ]]; then
  echo "name gate FAILED — 'silman' outside allowed locations:"
  echo "$hits"
  exit 1
fi
echo "name gate OK"
