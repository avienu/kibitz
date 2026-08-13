#!/usr/bin/env python3
"""Stamp the release version into the website's download section.

The download buttons in website/index.html point at versioned GitHub
release assets, so every release must rewrite them — this script is the
one place that knows how. The version comes from
app/src-tauri/tauri.conf.json (what the bundles actually report); the
tag defaults to "v" + that version and may be overridden for test tags
whose bundle version differs (e.g. --tag v0.1.0-test5 with version
0.1.0).

Run it BEFORE tagging (docs/RELEASING.md step 1); the release workflow
refuses stable tags whose site still points elsewhere.

Idempotent. Fails loudly if the expected patterns are not found, so a
future rewrite of index.html cannot silently orphan the stamping.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INDEX = ROOT / "website" / "index.html"
CONF = ROOT / "app" / "src-tauri" / "tauri.conf.json"


def main() -> int:
    tag = None
    args = sys.argv[1:]
    if args[:1] == ["--tag"] and len(args) == 2:
        tag = args[1]
    elif args:
        print(f"usage: {sys.argv[0]} [--tag vX.Y.Z[-suffix]]", file=sys.stderr)
        return 2

    version = json.loads(CONF.read_text())["version"]
    tag = tag or f"v{version}"
    if not tag.startswith("v"):
        print(f"tag must start with 'v': {tag}", file=sys.stderr)
        return 2

    html = INDEX.read_text()

    html, n_urls = re.subn(
        r"releases/download/v[^/]+/Kibitz_[0-9][^_]*_",
        f"releases/download/{tag}/Kibitz_{version}_",
        html,
    )
    html, n_text = re.subn(
        r"current build is <strong>v[^<]+</strong>",
        f"current build is <strong>{tag}</strong>",
        html,
    )

    if n_urls < 4 or n_text != 1:
        print(
            f"index.html no longer matches the stamping patterns "
            f"(rewrote {n_urls} asset links, {n_text} version mentions) — "
            f"update scripts/site_stamp_version.py alongside the page",
            file=sys.stderr,
        )
        return 1

    INDEX.write_text(html)
    print(f"stamped {tag} (bundle version {version}): {n_urls} links, {n_text} mention")
    return 0


if __name__ == "__main__":
    sys.exit(main())
