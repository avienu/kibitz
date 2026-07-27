import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown, spanText, splitSections } from "./markdown";

describe("parseInline", () => {
  it("passes plain text through", () => {
    expect(parseInline("hello world")).toEqual([{ text: "hello world" }]);
  });

  it("extracts code and bold spans", () => {
    expect(parseInline("run `cargo` with **care** now")).toEqual([
      { text: "run " },
      { text: "cargo", code: true },
      { text: " with " },
      { text: "care", bold: true },
      { text: " now" },
    ]);
  });

  it("does not treat ** inside code as bold", () => {
    expect(parseInline("`a ** b`")).toEqual([{ text: "a ** b", code: true }]);
  });
});

describe("parseMarkdown", () => {
  it("parses headings, paragraphs and rules", () => {
    const blocks = parseMarkdown("# Title\n\nSome text\njoined here.\n\n---\n");
    expect(blocks).toEqual([
      { kind: "heading", level: 1, spans: [{ text: "Title" }] },
      { kind: "para", spans: [{ text: "Some text joined here." }] },
      { kind: "rule" },
    ]);
  });

  it("parses fenced code blocks verbatim", () => {
    const blocks = parseMarkdown("```\ncargo run -- --db x.sqlite stats\n```\n");
    expect(blocks).toEqual([{ kind: "code", text: "cargo run -- --db x.sqlite stats" }]);
  });

  it("parses lists with hanging-indent continuations", () => {
    const blocks = parseMarkdown("- first item\n  continues here\n- second\n");
    expect(blocks).toEqual([
      {
        kind: "list",
        ordered: false,
        items: [[{ text: "first item continues here" }], [{ text: "second" }]],
      },
    ]);
  });

  it("parses ordered lists separately from unordered", () => {
    const blocks = parseMarkdown("1. one\n2. two\n\n- bullet\n");
    expect(blocks).toEqual([
      { kind: "list", ordered: true, items: [[{ text: "one" }], [{ text: "two" }]] },
      { kind: "list", ordered: false, items: [[{ text: "bullet" }]] },
    ]);
  });
});

describe("splitSections (Help TOC, round 2)", () => {
  const FIXTURE = [
    "# Kibitz User Guide",
    "",
    "Intro paragraph before any section.",
    "",
    "## The Game view",
    "",
    "Body prose.",
    "",
    "### Keyboard map",
    "",
    "- **←/→** step",
    "",
    "## CLI-only features",
    "",
    "```",
    "kibitz-cli --db x.sqlite stats",
    "```",
    "",
  ].join("\n");

  it("splits at every h1/h2 and keeps deeper headings inside", () => {
    const sections = splitSections(parseMarkdown(FIXTURE));
    expect(sections.map((s) => s.title)).toEqual([
      "Kibitz User Guide",
      "The Game view",
      "CLI-only features",
    ]);
    // h3 stays inside its section; the section heading itself is excluded.
    const game = sections[1];
    expect(game.blocks[0]).toEqual({ kind: "para", spans: [{ text: "Body prose." }] });
    expect(game.blocks.some((b) => b.kind === "heading" && b.level === 3)).toBe(true);
    expect(game.blocks.some((b) => b.kind === "heading" && b.level <= 2)).toBe(false);
    // Code blocks land in their section (the CLI card).
    expect(sections[2].blocks).toEqual([
      { kind: "code", text: "kibitz-cli --db x.sqlite stats" },
    ]);
  });

  it("collects blocks before any heading under the lead title", () => {
    const sections = splitSections(parseMarkdown("plain intro\n\n## A\n\nbody\n"), "Overview");
    expect(sections.map((s) => s.title)).toEqual(["Overview", "A"]);
    expect(sections[0].blocks).toHaveLength(1);
  });

  it("spanText flattens spans for TOC labels", () => {
    expect(spanText([{ text: "a " }, { text: "b", bold: true }])).toBe("a b");
  });
});
