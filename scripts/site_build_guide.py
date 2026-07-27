#!/usr/bin/env python3
"""Build website/guide.html from docs/USER_GUIDE.md (run-8 item 4).

Stdlib only, no generators. The markdown subset is a faithful port of the
app's in-house renderer (app/src/lib/markdown.ts): ATX headings (# .. ####),
paragraphs, unordered (-) and ordered (1.) lists with hanging-indent
continuation lines, fenced code blocks, horizontal rules, and inline
`code` / **bold** spans (code wins over bold; neither nests).

Deterministic: the output is a pure function of docs/USER_GUIDE.md and this
script. The generated website/guide.html is committed; CI rebuilds it and
diffs against the checked-in copy.

Usage: python3 scripts/site_build_guide.py [--check]
  --check  build to memory and exit 1 if website/guide.html differs
"""

from __future__ import annotations

import html
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "docs" / "USER_GUIDE.md"
OUT = REPO / "website" / "guide.html"

# ---------------------------------------------------------------------------
# Parsing — ported line-for-line from app/src/lib/markdown.ts.
# ---------------------------------------------------------------------------

HEADING = re.compile(r"^(#{1,4})\s+(.*)$")
UL_ITEM = re.compile(r"^-\s+(.*)$")
OL_ITEM = re.compile(r"^\d+\.\s+(.*)$")
RULE = re.compile(r"^---+\s*$")
CONTINUATION = re.compile(r"^\s+\S")
# Code spans win over bold; neither nests inside the other.
INLINE = re.compile(r"`([^`]*)`|\*\*([^*]+)\*\*")


def parse_inline(text: str) -> list[dict]:
    """Split text into plain / `code` / **bold** spans (no nesting)."""
    out: list[dict] = []
    last = 0
    for m in INLINE.finditer(text):
        if m.start() > last:
            out.append({"text": text[last : m.start()]})
        if m.group(1) is not None:
            out.append({"text": m.group(1), "code": True})
        else:
            out.append({"text": m.group(2), "bold": True})
        last = m.end()
    if last < len(text):
        out.append({"text": text[last:]})
    return out


def span_text(spans: list[dict]) -> str:
    return "".join(s["text"] for s in spans)


def parse_markdown(src: str) -> list[dict]:
    """Parse a markdown document into a flat list of blocks."""
    blocks: list[dict] = []
    lines = src.split("\n")
    i = 0

    def take_list(ordered: bool) -> dict:
        """Consume list items of one kind, folding indented continuations."""
        nonlocal i
        item_re = OL_ITEM if ordered else UL_ITEM
        items: list[list[dict]] = []
        cur: str | None = None
        while i < len(lines):
            line = lines[i]
            m = item_re.match(line)
            if m:
                if cur is not None:
                    items.append(parse_inline(cur))
                cur = m.group(1)
                i += 1
            elif cur is not None and CONTINUATION.match(line):
                cur += f" {line.strip()}"  # hanging-indent continuation
                i += 1
            else:
                break
        if cur is not None:
            items.append(parse_inline(cur))
        return {"kind": "list", "ordered": ordered, "items": items}

    while i < len(lines):
        line = lines[i]
        if line.strip() == "":
            i += 1
            continue
        if line.startswith("```"):
            i += 1
            buf: list[str] = []
            while i < len(lines) and not lines[i].startswith("```"):
                buf.append(lines[i])
                i += 1
            i += 1  # closing fence (or EOF)
            blocks.append({"kind": "code", "text": "\n".join(buf)})
            continue
        if RULE.match(line):
            blocks.append({"kind": "rule"})
            i += 1
            continue
        h = HEADING.match(line)
        if h:
            blocks.append(
                {"kind": "heading", "level": len(h.group(1)), "spans": parse_inline(h.group(2))}
            )
            i += 1
            continue
        if UL_ITEM.match(line):
            blocks.append(take_list(False))
            continue
        if OL_ITEM.match(line):
            blocks.append(take_list(True))
            continue
        # Paragraph: join consecutive plain lines.
        buf = []
        while i < len(lines):
            l = lines[i]
            if (
                l.strip() == ""
                or l.startswith("```")
                or RULE.match(l)
                or HEADING.match(l)
                or UL_ITEM.match(l)
                or OL_ITEM.match(l)
            ):
                break
            buf.append(l.strip())
            i += 1
        blocks.append({"kind": "para", "spans": parse_inline(" ".join(buf))})
    return blocks


# ---------------------------------------------------------------------------
# Rendering.
# ---------------------------------------------------------------------------


def render_spans(spans: list[dict]) -> str:
    out = []
    for s in spans:
        text = html.escape(s["text"], quote=False)
        if s.get("code"):
            out.append(f"<code>{text}</code>")
        elif s.get("bold"):
            out.append(f"<strong>{text}</strong>")
        else:
            out.append(text)
    return "".join(out)


def make_slugger():
    seen: dict[str, int] = {}

    def slug(title: str) -> str:
        base = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "section"
        n = seen.get(base, 0)
        seen[base] = n + 1
        return base if n == 0 else f"{base}-{n + 1}"

    return slug


def render_blocks(blocks: list[dict]) -> tuple[str, list[tuple[str, str]]]:
    """Render blocks to HTML; return (html, [(slug, title)] for h2 TOC)."""
    slug = make_slugger()
    toc: list[tuple[str, str]] = []
    out: list[str] = []
    for b in blocks:
        kind = b["kind"]
        if kind == "heading":
            title = span_text(b["spans"])
            anchor = slug(title)
            if b["level"] == 2:
                toc.append((anchor, title))
            out.append(
                f'<h{b["level"]} id="{anchor}">{render_spans(b["spans"])}</h{b["level"]}>'
            )
        elif kind == "para":
            out.append(f"<p>{render_spans(b['spans'])}</p>")
        elif kind == "code":
            out.append(f"<pre><code>{html.escape(b['text'], quote=False)}</code></pre>")
        elif kind == "list":
            tag = "ol" if b["ordered"] else "ul"
            items = "\n".join(f"<li>{render_spans(it)}</li>" for it in b["items"])
            out.append(f"<{tag}>\n{items}\n</{tag}>")
        elif kind == "rule":
            out.append("<hr>")
    return "\n".join(out), toc


PAGE = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Kibitz User Guide</title>
  <meta name="description" content="The Kibitz user guide: every screen, every feature, and every CLI-only command of the open-source desktop chess coach.">
  <link rel="stylesheet" href="style.css">
</head>
<body>

<header class="site-header">
  <div class="site-header-inner">
    <a class="wordmark" href="index.html">KIBITZ</a>
    <nav class="site-nav">
      <a href="index.html">Home</a>
      <a href="index.html#download">Download</a>
      <a href="https://github.com/avienu/kibitz">Source</a>
    </nav>
  </div>
</header>

<main class="guide-layout">
  <nav class="guide-toc" aria-label="Guide sections">
    <p class="kicker">Contents</p>
{toc}
  </nav>
  <article class="guide-body">
{body}
  </article>
</main>

<footer class="site-footer">
  <div class="site-footer-inner">
    <p>Kibitz — free software. Application: GPL-3.0. Core libraries: BSD-3-Clause.
    <a href="https://github.com/avienu/kibitz">Source on GitHub</a> ·
    <a href="index.html">Home</a></p>
    <p>This page is generated from docs/USER_GUIDE.md by scripts/site_build_guide.py —
    the same guide the app shows under Help &amp; tour.</p>
  </div>
</footer>

</body>
</html>
"""


def build() -> str:
    blocks = parse_markdown(SRC.read_text(encoding="utf-8"))
    body, toc = render_blocks(blocks)
    toc_html = "\n".join(
        f'    <a href="#{anchor}">{html.escape(title, quote=False)}</a>' for anchor, title in toc
    )
    return PAGE.format(toc=toc_html, body=body)


def main() -> int:
    page = build()
    if "--check" in sys.argv[1:]:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else None
        if current != page:
            print(f"STALE: {OUT} does not match the output of {Path(__file__).name}")
            return 1
        print(f"OK: {OUT} is up to date")
        return 0
    OUT.write_text(page, encoding="utf-8")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
