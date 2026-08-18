#!/usr/bin/env python3
"""Era study: recall per axis, bucketed by the cited game's year.

Joins `kibitz-cli book-eval --verbose` (which prints the corpus
fingerprints it ran on) with the gitignored corpus JSON. Only entries
whose citation carries a game year (1800-1999) are bucketed; the rest
are reported as undated. Bucket sizes are printed beside every figure
per the corpus-composition rule (docs/VALIDATION.md).

Usage: python3 scripts/era_study.py [corpus-dir] [kibitz-cli]
"""

import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

CORPUS = Path(sys.argv[1] if len(sys.argv) > 1 else "testdata/private/book-trials")
CLI = sys.argv[2] if len(sys.argv) > 2 else "./target/release/kibitz-cli"

BUCKETS = [(0, 1899, "<=1899"), (1900, 1909, "1900-09"), (1910, 1919, "1910-19"), (1920, 1999, "1920-28")]


def bucket_of(year):
    for lo, hi, name in BUCKETS:
        if lo <= year <= hi:
            return name
    return None


def main():
    # Scorable expectations mirror book-eval exactly: plan tags outside
    # KNOWN_HINTS are vocabulary gaps (not in the denominator), and a
    # "balanced" favors expectation is not scored.
    known = set(
        re.findall(
            r'"(\w+)"',
            re.search(
                r"const KNOWN_HINTS.*?\n\];",
                Path("app/kibitz-db/src/bookeval.rs").read_text(),
                re.S,
            ).group(0),
        )
    )
    entries = {}
    for f in sorted(CORPUS.glob("*.json")):
        for e in json.loads(f.read_text())["positions"]:
            yrs = [int(y) for y in re.findall(r"\b(1[89]\d\d)\b", e.get("citation", ""))]
            exp = e.get("expected", {})
            entries[e["id"]] = {
                "year": max(yrs) if yrs else None,
                "imb": len(exp.get("imbalances", [])),
                "plan": sum(1 for t in exp.get("plan_tags", []) if t in known),
                "favors": 1 if exp.get("favors") not in (None, "balanced") else 0,
                "suggest": 1 if exp.get("best_moves") else 0,
            }

    out = subprocess.run([CLI, "book-eval", "--verbose", str(CORPUS)], capture_output=True, text=True).stdout
    print("\n".join(l for l in out.splitlines() if l.startswith("input") or "combined" in l))

    missed = defaultdict(lambda: defaultdict(int))
    at3_missed = set()
    for line in out.splitlines():
        m = re.match(r"\s*MISS (\w+)\s+([\w./-]+):", line)
        if not m:
            continue
        axis, eid = m.group(1), m.group(2)
        missed[eid][axis] += 1
        if axis == "suggest":
            at3_missed.add(eid)

    # Vocabulary-gap plan tags never count as scorable in book-eval's
    # plans axis; verbose MISS plan lines cover only KNOWN_HINTS misses,
    # so per-entry expected-plan counts must exclude gap tags. We proxy
    # by clamping hits at zero.
    agg = defaultdict(lambda: defaultdict(lambda: [0, 0]))  # bucket -> axis -> [hit, total]
    for eid, e in entries.items():
        b = bucket_of(e["year"]) if e["year"] else "undated"
        for axis, key in [("imbalance", "imb"), ("plan", "plan"), ("favors", "favors")]:
            total = e[key]
            if total == 0:
                continue
            hit = max(0, total - missed[eid][axis])
            agg[b][axis][0] += hit
            agg[b][axis][1] += total
        if e["suggest"]:
            agg[b]["suggest@3"][0] += 0 if eid in at3_missed else 1
            agg[b]["suggest@3"][1] += 1
        agg[b]["entries"][0] += 1
        agg[b]["entries"][1] += 1

    order = [n for _, _, n in BUCKETS] + ["undated"]
    print(f"\n{'bucket':>8} {'n':>4}  " + "".join(f"{a:>16}" for a in ["imbalance", "plan", "favors", "suggest@3"]))
    for b in order:
        if b not in agg:
            continue
        n = agg[b]["entries"][0]
        row = f"{b:>8} {n:>4}  "
        for axis in ["imbalance", "plan", "favors", "suggest@3"]:
            hit, tot = agg[b][axis]
            row += f"{hit:>4}/{tot:<4}={100*hit/tot if tot else 0:5.1f}% " if tot else f"{'—':>16}"
        print(row)


if __name__ == "__main__":
    main()
