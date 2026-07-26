import { describe, expect, it } from "vitest";
import type { AnalysisRow } from "./analyses";
import {
  classifyVariation,
  gameEngines,
  movesRows,
  movesRowsFromSans,
  nagTone,
  type MovesRow,
} from "./movesView";
import { buildAnnView, type JsonToken } from "./tokens";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const NO_ENGINES = { fresh: [], legacy: [] };

const mv = (san: string): JsonToken => ({ t: "move", san });

function pairs(rows: MovesRow[]) {
  return rows.filter((r) => r.kind === "pair");
}

describe("movesRows (annotated stream → pair grid)", () => {
  it("pairs white and black with move numbers", () => {
    const view = buildAnnView(START, [mv("e4"), mv("e5"), mv("Nf3")]);
    const rows = movesRows(view, START, NO_ENGINES);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({
      kind: "pair",
      num: 1,
      white: { san: "e4", ply: 1 },
      black: { san: "e5", ply: 2 },
    });
    expect(rows[1]).toMatchObject({ kind: "pair", num: 2, white: { san: "Nf3" }, black: null });
  });

  it("a comment breaks the pair; black continues with an ellipsis cell", () => {
    const tokens: JsonToken[] = [mv("e4"), { t: "comment", text: "best by test" }, mv("e5")];
    const rows = movesRows(buildAnnView(START, tokens), START, NO_ENGINES);
    expect(rows.map((r) => r.kind)).toEqual(["pair", "comment", "pair"]);
    expect(rows[1]).toMatchObject({ kind: "comment", text: "best by test" });
    expect(rows[2]).toMatchObject({ kind: "pair", num: 1, whiteEllipsis: true, black: { san: "e5" } });
  });

  it("captures NAGs onto the move cell", () => {
    const tokens: JsonToken[] = [mv("e4"), { t: "nag", value: 6 }, mv("e5")];
    const rows = movesRows(buildAnnView(START, tokens), START, NO_ENGINES);
    const p = pairs(rows)[0] as Extract<MovesRow, { kind: "pair" }>;
    expect(p.white?.nag).toBe(6);
  });

  it("renders a variation row after its move and before black's row", () => {
    const tokens: JsonToken[] = [
      mv("e4"),
      { t: "varStart" },
      mv("d4"),
      mv("d5"),
      { t: "varEnd" },
      mv("e5"),
    ];
    const rows = movesRows(buildAnnView(START, tokens), START, NO_ENGINES);
    expect(rows.map((r) => r.kind)).toEqual(["pair", "variation", "pair"]);
    const v = rows[1] as Extract<MovesRow, { kind: "variation" }>;
    expect(v.line).toBe("1. d4 d5");
    expect(v.style).toBe("plain");
    expect(v.tag).toBe("VARIATION");
    expect(rows[2]).toMatchObject({ kind: "pair", whiteEllipsis: true, black: { san: "e5" } });
  });

  it("fresh variations list before legacy at the same move", () => {
    const engines = { fresh: ["Stockfish 18"], legacy: ["Deep Rybka 4 x64 (2011)"] };
    const tokens: JsonToken[] = [
      mv("e4"),
      { t: "varStart" },
      { t: "comment", text: "Deep Rybka 4: +0.31" },
      mv("d4"),
      { t: "varEnd" },
      { t: "varStart" },
      { t: "comment", text: "Stockfish 18 +0.35/24" },
      mv("Nf3"),
      { t: "varEnd" },
    ];
    const rows = movesRows(buildAnnView(START, tokens), START, engines);
    const vars = rows.filter((r) => r.kind === "variation") as Extract<
      MovesRow,
      { kind: "variation" }
    >[];
    expect(vars.map((v) => v.style)).toEqual(["fresh", "legacy"]);
  });
});

describe("movesRowsFromSans (plain games)", () => {
  it("builds simple pairs", () => {
    const rows = movesRowsFromSans(["e4", "e5", "Nf3", "Nc6"], START);
    expect(rows).toHaveLength(2);
    expect(rows[1]).toMatchObject({ num: 2, white: { san: "Nf3", ply: 3 }, black: { san: "Nc6", ply: 4 } });
  });

  it("black-to-move start FEN begins with an ellipsis row", () => {
    const fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    const rows = movesRowsFromSans(["e5", "Nf3"], fen);
    expect(rows[0]).toMatchObject({ kind: "pair", num: 1, whiteEllipsis: true, black: { san: "e5", ply: 1 } });
    expect(rows[1]).toMatchObject({ kind: "pair", num: 2, white: { san: "Nf3", ply: 2 } });
  });
});

describe("classifyVariation", () => {
  const engines = { fresh: ["Stockfish 18"], legacy: ["Deep Rybka 4 x64"] };

  it("legacy engine mention → LEGACY with year when present", () => {
    const r = classifyVariation(["Deep Rybka 4 x64: -0.51 (2011)"], engines);
    expect(r).toEqual({ style: "legacy", tag: "LEGACY 2011" });
  });

  it("bare pre-2020 year → legacy", () => {
    expect(classifyVariation(["+0.2, 2011 import"], NO_ENGINES).style).toBe("legacy");
  });

  it("fresh engine mention → ENGINE with depth when parseable", () => {
    expect(classifyVariation(["Stockfish 18 +0.35/24"], engines)).toEqual({
      style: "fresh",
      tag: "ENGINE d24",
    });
  });

  it("no engine markers → plain VARIATION", () => {
    expect(classifyVariation(["the thematic break"], engines)).toEqual({
      style: "plain",
      tag: "VARIATION",
    });
    expect(classifyVariation([], engines).style).toBe("plain");
  });

  it("gameEngines splits identities by kind", () => {
    const rows = [
      { ply: 1, kind: "fresh", engine: "Stockfish 18", depth: null, nodes: 1, evalCp: 0, createdAt: "" },
      { ply: 2, kind: "legacy-import", engine: "Rybka", depth: null, nodes: null, evalCp: 0, createdAt: "" },
    ] satisfies AnalysisRow[];
    expect(gameEngines(rows)).toEqual({ fresh: ["Stockfish 18"], legacy: ["Rybka"] });
  });
});

describe("nagTone", () => {
  it("? family bad, ! family accent", () => {
    expect(nagTone(2)).toBe("bad");
    expect(nagTone(4)).toBe("bad");
    expect(nagTone(6)).toBe("bad");
    expect(nagTone(1)).toBe("accent");
    expect(nagTone(3)).toBe("accent");
    expect(nagTone(5)).toBe("accent");
    expect(nagTone(10)).toBe("plain");
  });
});
