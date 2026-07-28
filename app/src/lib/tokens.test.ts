import { describe, expect, it } from "vitest";
import {
  buildAnnView,
  commentAfter,
  cycleNag,
  deleteComment,
  deleteVariation,
  insertVariation,
  mainlineMoveTokenIndex,
  nagSuffix,
  setComment,
  setNag,
  type JsonToken,
  type MoveItem,
} from "./tokens";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const m = (san: string): JsonToken => ({ t: "move", san });
const n = (value: number): JsonToken => ({ t: "nag", value });
const c = (text: string): JsonToken => ({ t: "comment", text });
const vs: JsonToken = { t: "varStart" };
const ve: JsonToken = { t: "varEnd" };

/** 1. e4 {best by test} e5 (1... c5! 2. Nf3 (2. c3)) 2. Nf3 */
const ANNOTATED: JsonToken[] = [
  c("pre-game"),
  m("e4"),
  c("best by test"),
  m("e5"),
  vs,
  m("c5"),
  n(1),
  m("Nf3"),
  vs,
  m("c3"),
  ve,
  ve,
  m("Nf3"),
];

const moveItems = (tokens: JsonToken[]) =>
  buildAnnView(START, tokens).items.filter((it): it is MoveItem => it.kind === "move");

describe("buildAnnView", () => {
  it("renders mainline, comments, NAGs and nested variations", () => {
    const view = buildAnnView(START, ANNOTATED);
    expect(view.error).toBeNull();
    expect(view.mainlineSans).toEqual(["e4", "e5", "Nf3"]);

    const moves = view.items.filter((it): it is MoveItem => it.kind === "move");
    expect(moves.map((mv) => [mv.san, mv.num, mv.depth, mv.mainlinePly])).toEqual([
      ["e4", "1.", 0, 1],
      ["e5", "1...", 0, 2], // comment interrupted: black gets its number
      ["c5", "1...", 1, null], // variation replaces 1... e5 -> branches after 1. e4
      ["Nf3", "2.", 1, null],
      ["c3", "2.", 2, null], // nested variation replaces 2. Nf3
      ["Nf3", "2.", 0, 3],
    ]);

    // The variation's c5 parsed from the position after 1. e4 (branch from
    // before the last move), not after 1... e5.
    const c5 = moves[2];
    expect(c5.fenAfter).toContain("2p5/4P3"); // c5+e4 pawn structure present
    expect(c5.nag).toBe(1);

    const comments = view.items.filter((it) => it.kind === "comment");
    expect(comments).toHaveLength(2);
    expect(comments[0]).toMatchObject({ text: "pre-game", depth: 0 });
    expect(comments[1]).toMatchObject({ text: "best by test", depth: 0 });

    const parens = view.items.filter((it) => it.kind === "varStart" || it.kind === "varEnd");
    expect(parens.map((p) => [p.kind, p.depth])).toEqual([
      ["varStart", 1],
      ["varStart", 2],
      ["varEnd", 2],
      ["varEnd", 1],
    ]);
  });

  it("reports illegal SAN and structural errors", () => {
    expect(buildAnnView(START, [m("Qxg8")]).error).toContain("Qxg8");
    expect(buildAnnView(START, [vs, m("e4"), ve]).error).toContain("Variation before any move");
    expect(buildAnnView(START, [m("e4"), ve]).error).toContain("varEnd without varStart");
    expect(buildAnnView(START, [m("e4"), vs, m("d4")]).error).toContain("Unclosed variation");
  });

  it("consecutive variations both replace the same move", () => {
    // 1. e4 e5 (1... c5) (1... e6) — both must parse after 1. e4.
    const tokens = [m("e4"), m("e5"), vs, m("c5"), ve, vs, m("e6"), ve];
    const view = buildAnnView(START, tokens);
    expect(view.error).toBeNull();
    expect(view.mainlineSans).toEqual(["e4", "e5"]);
    const moves = moveItems(tokens);
    expect(moves.map((mv) => [mv.san, mv.num])).toEqual([
      ["e4", "1."],
      ["e5", ""], // uninterrupted black reply: no number prefix
      ["c5", "1..."],
      ["e6", "1..."],
    ]);
  });
});

describe("insertVariation", () => {
  const plain = [m("e4"), m("e5"), m("Nf3")];

  it("inserts VarStart+move+VarEnd after the mainline move", () => {
    const next = insertVariation(plain, 2, ["c5"]);
    expect(next).toEqual([m("e4"), m("e5"), vs, m("c5"), ve, m("Nf3")]);
    expect(buildAnnView(START, next).error).toBeNull();
    // The original array is untouched (pure transform).
    expect(plain).toHaveLength(3);
  });

  it("appends after existing annotations and variations of the same move", () => {
    const withVar = insertVariation(plain, 2, ["c5"]);
    const withBoth = insertVariation(withVar, 2, ["e6"]);
    expect(withBoth).toEqual([m("e4"), m("e5"), vs, m("c5"), ve, vs, m("e6"), ve, m("Nf3")]);
    expect(buildAnnView(START, withBoth).error).toBeNull();

    const annotated = [m("e4"), n(1), c("note"), m("e5")];
    expect(insertVariation(annotated, 1, ["d4"])).toEqual([
      m("e4"),
      n(1),
      c("note"),
      vs,
      m("d4"),
      ve,
      m("e5"),
    ]);
  });

  it("stores an optional tag comment inside the variation (engine lines)", () => {
    const next = insertVariation(plain, 2, ["c5", "Nf3"], "ENGINE d24 +0.53");
    expect(next).toEqual([
      m("e4"),
      m("e5"),
      vs,
      m("c5"),
      m("Nf3"),
      c("ENGINE d24 +0.53"),
      ve,
      m("Nf3"),
    ]);
    expect(buildAnnView(START, next).error).toBeNull();
  });

  it("counts only mainline moves when locating the ply", () => {
    const tokens = [m("e4"), m("e5"), vs, m("c5"), m("Nf3"), ve, m("Nf3")];
    // Mainline ply 3 is the LAST Nf3 (token 6), not the variation's.
    expect(mainlineMoveTokenIndex(tokens, 3)).toBe(6);
    const next = insertVariation(tokens, 3, ["Nc3"]);
    expect(next.slice(7)).toEqual([vs, m("Nc3"), ve]);
  });

  it("is a no-op for a missing ply or empty variation", () => {
    expect(insertVariation(plain, 9, ["c5"])).toEqual(plain);
    expect(insertVariation(plain, 1, [])).toEqual(plain);
  });
});

describe("deleteVariation", () => {
  it("removes the whole group including nested subvariations", () => {
    const varStartIndex = ANNOTATED.findIndex((t) => t.t === "varStart");
    const next = deleteVariation(ANNOTATED, varStartIndex);
    expect(next).toEqual([c("pre-game"), m("e4"), c("best by test"), m("e5"), m("Nf3")]);
  });

  it("ignores non-varStart indices", () => {
    expect(deleteVariation(ANNOTATED, 0)).toEqual(ANNOTATED);
  });
});

describe("setComment / deleteComment", () => {
  const plain = [m("e4"), n(1), m("e5")];

  it("inserts a comment after the move's NAGs", () => {
    expect(setComment(plain, 0, "best by test")).toEqual([
      m("e4"),
      n(1),
      c("best by test"),
      m("e5"),
    ]);
  });

  it("replaces an existing comment and deletes on empty text", () => {
    const withComment = setComment(plain, 0, "old");
    expect(setComment(withComment, 0, "new")).toEqual([m("e4"), n(1), c("new"), m("e5")]);
    expect(setComment(withComment, 0, "  ")).toEqual(plain);
    expect(setComment(plain, 0, "")).toEqual(plain);
  });

  it("commentAfter finds the attached comment through NAGs", () => {
    const withComment = setComment(plain, 0, "note");
    expect(commentAfter(withComment, 0)).toEqual({ index: 2, text: "note" });
    expect(commentAfter(plain, 0)).toBeNull();
  });

  it("deleteComment removes exactly the comment token", () => {
    const withComment = setComment(plain, 0, "note");
    expect(deleteComment(withComment, 2)).toEqual(plain);
    expect(deleteComment(plain, 0)).toEqual(plain); // not a comment: no-op
  });
});

describe("cycleNag", () => {
  it("cycles none -> ! -> ? -> !! -> ?? -> !? -> ?! -> none", () => {
    let tokens: JsonToken[] = [m("e4"), m("e5")];
    const seen: (number | null)[] = [];
    for (let i = 0; i < 7; i++) {
      tokens = cycleNag(tokens, 0);
      const t = tokens[1];
      seen.push(t.t === "nag" ? t.value : null);
    }
    expect(seen).toEqual([1, 2, 3, 4, 5, 6, null]);
    expect(tokens).toEqual([m("e4"), m("e5")]);
  });

  it("clears an out-of-cycle NAG and renders unknown NAGs as $n", () => {
    const tokens = [m("e4"), n(42), m("e5")];
    expect(cycleNag(tokens, 0)).toEqual([m("e4"), m("e5")]);
    expect(nagSuffix(42)).toBe(" $42");
    expect(nagSuffix(3)).toBe("!!");
    expect(nagSuffix(null)).toBe("");
  });
});

describe("setNag (picker direct selection)", () => {
  it("sets, replaces and clears the NAG after a move", () => {
    const plain: JsonToken[] = [m("e4"), m("e5")];
    expect(setNag(plain, 0, 5)).toEqual([m("e4"), n(5), m("e5")]);
    expect(setNag([m("e4"), n(1), m("e5")], 0, 4)).toEqual([m("e4"), n(4), m("e5")]);
    expect(setNag([m("e4"), n(1), m("e5")], 0, null)).toEqual(plain);
    expect(setNag(plain, 0, null)).toEqual(plain); // clearing nothing: no-op
  });
});
