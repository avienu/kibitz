/**
 * Minimal markdown parser for the in-app Help viewer (run-5 item 6).
 * Supports exactly what docs/USER_GUIDE.md uses: ATX headings (# … ####),
 * paragraphs, unordered (-) and ordered (1.) lists, fenced code blocks,
 * horizontal rules, and inline `code` / **bold** spans.
 *
 * Pure data-in/data-out (no React, no DOM) — unit-testable; the Help
 * component maps the block structure to elements.
 */

export interface InlineSpan {
  text: string;
  code?: boolean;
  bold?: boolean;
}

export type Block =
  | { kind: "heading"; level: number; spans: InlineSpan[] }
  | { kind: "para"; spans: InlineSpan[] }
  | { kind: "code"; text: string }
  | { kind: "list"; ordered: boolean; items: InlineSpan[][] }
  /** A line that is exactly `![alt](src)` — a screenshot with its caption.
   * Block-level only: an image inside a sentence would have to reflow with
   * the text, and the guide never wants one. */
  | { kind: "figure"; src: string; alt: string }
  | { kind: "rule" };

/** Split text into plain / `code` / **bold** spans (no nesting). */
export function parseInline(text: string): InlineSpan[] {
  const out: InlineSpan[] = [];
  // Code spans win over bold; neither nests inside the other.
  const re = /`([^`]*)`|\*\*([^*]+)\*\*/g;
  let last = 0;
  for (let m = re.exec(text); m !== null; m = re.exec(text)) {
    if (m.index > last) out.push({ text: text.slice(last, m.index) });
    if (m[1] !== undefined) out.push({ text: m[1], code: true });
    else out.push({ text: m[2], bold: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

/** Plain text of a span list (TOC labels, section titles). */
export function spanText(spans: InlineSpan[]): string {
  return spans.map((s) => s.text).join("");
}

/** One Help-reader section: an h1/h2 title plus everything under it. */
export interface GuideSection {
  title: string;
  /** Blocks of the section, heading excluded (the reader renders the
   * title itself at h2 scale). */
  blocks: Block[];
}

/**
 * Split parsed blocks into reader sections at every level-1/2 heading
 * (round-2 Help TOC). Deeper headings (###, ####) stay inside their
 * section. Blocks before any heading land in a leading section titled
 * `leadTitle` (only emitted when such blocks exist).
 */
export function splitSections(
  blocks: Block[],
  leadTitle = "Overview",
): GuideSection[] {
  const sections: GuideSection[] = [];
  let cur: GuideSection | null = null;
  for (const b of blocks) {
    if (b.kind === "heading" && b.level <= 2) {
      cur = { title: spanText(b.spans), blocks: [] };
      sections.push(cur);
      continue;
    }
    if (!cur) {
      cur = { title: leadTitle, blocks: [] };
      sections.push(cur);
    }
    cur.blocks.push(b);
  }
  return sections;
}

const HEADING = /^(#{1,4})\s+(.*)$/;
/** `![alt](src)` alone on a line. Alt text is required — it is both the
 * caption and what a screen reader gets, and a screenshot with neither is
 * decoration the guide should not be carrying. */
const FIGURE = /^!\[([^\]]+)\]\(([^)\s]+)\)\s*$/;
const UL_ITEM = /^-\s+(.*)$/;
const OL_ITEM = /^\d+\.\s+(.*)$/;

/** Parse a markdown document into a flat list of blocks. */
export function parseMarkdown(src: string): Block[] {
  const blocks: Block[] = [];
  const lines = src.split("\n");
  let i = 0;

  /** Consume list items of one kind, folding indented continuation lines. */
  const takeList = (ordered: boolean): Block => {
    const itemRe = ordered ? OL_ITEM : UL_ITEM;
    const items: InlineSpan[][] = [];
    let cur: string | null = null;
    while (i < lines.length) {
      const line = lines[i];
      const m = itemRe.exec(line);
      if (m) {
        if (cur !== null) items.push(parseInline(cur));
        cur = m[1];
        i++;
      } else if (cur !== null && /^\s+\S/.test(line)) {
        cur += ` ${line.trim()}`; // hanging-indent continuation
        i++;
      } else {
        break;
      }
    }
    if (cur !== null) items.push(parseInline(cur));
    return { kind: "list", ordered, items };
  };

  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i++;
      continue;
    }
    if (line.startsWith("```")) {
      i++;
      const buf: string[] = [];
      while (i < lines.length && !lines[i].startsWith("```")) {
        buf.push(lines[i]);
        i++;
      }
      i++; // closing fence (or EOF)
      blocks.push({ kind: "code", text: buf.join("\n") });
      continue;
    }
    const fig = FIGURE.exec(line);
    if (fig) {
      blocks.push({ kind: "figure", alt: fig[1], src: fig[2] });
      i++;
      continue;
    }
    if (/^---+\s*$/.test(line)) {
      blocks.push({ kind: "rule" });
      i++;
      continue;
    }
    const h = HEADING.exec(line);
    if (h) {
      blocks.push({
        kind: "heading",
        level: h[1].length,
        spans: parseInline(h[2]),
      });
      i++;
      continue;
    }
    if (UL_ITEM.test(line)) {
      blocks.push(takeList(false));
      continue;
    }
    if (OL_ITEM.test(line)) {
      blocks.push(takeList(true));
      continue;
    }
    // Paragraph: join consecutive plain lines.
    const buf: string[] = [];
    while (i < lines.length) {
      const l = lines[i];
      if (
        l.trim() === "" ||
        l.startsWith("```") ||
        /^---+\s*$/.test(l) ||
        HEADING.test(l) ||
        UL_ITEM.test(l) ||
        OL_ITEM.test(l)
      ) {
        break;
      }
      buf.push(l.trim());
      i++;
    }
    blocks.push({ kind: "para", spans: parseInline(buf.join(" ")) });
  }
  return blocks;
}
