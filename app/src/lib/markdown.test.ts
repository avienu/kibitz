import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown } from "./markdown";

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
