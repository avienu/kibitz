#!/usr/bin/env bash
# Regenerate docs/CORPUS_MANIFEST.md — fingerprints of the git-ignored
# measurement inputs.
#
# Two layers (second-desk feedback, run 12): SHA-256 for verification
# without the binary, and the runtime FNV-64 values that every eval
# PRINTS UNPROMPTED at the top of its output. The manifest is therefore
# cross-checkable rather than authoritative: if a measurement's printed
# fingerprints disagree with this file at the same ref, the manifest is
# stale and the printed values win. Regenerate in the same commit as
# any corpus change; a CI or pre-commit diff makes the omission loud.
set -euo pipefail
cd "$(dirname "$0")/.."
out=docs/CORPUS_MANIFEST.md
bin=target/release/kibitz-cli
[ -x "$bin" ] || cargo build --release -p kibitz-db --bin kibitz-cli
{
  echo "# Corpus manifest"
  echo
  echo "Fingerprints of the git-ignored measurement inputs. Every eval"
  echo "prints the FNV-64 values below unprompted, so a measurement"
  echo "self-identifies its inputs; SHA-256 is for verification without"
  echo "the binary. If a printed fingerprint disagrees with this file at"
  echo "the same ref, the manifest is stale and the printed value wins."
  echo
  echo '## Runtime fingerprints (as printed by every eval)'
  echo '```'
  "$bin" corpus-fingerprint
  echo '```'
  echo
  echo '## SHA-256'
  echo '```'
  for f in testdata/private/book-trials/*.json testdata/corpus/quiet_fens.txt; do
    printf '%s  %s\n' "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$f"
  done
  echo '```'
} > "$out"
echo "wrote $out"
