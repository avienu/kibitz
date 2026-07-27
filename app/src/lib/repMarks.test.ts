import { describe, expect, it } from "vitest";
import { repGlyphsByPly, type RepertoireMark } from "./repMarks";

const mark = (
  ply: number,
  color: "white" | "black",
  expectedSan: string,
  played: "matched" | "deviated",
): RepertoireMark => ({ ply, color, expectedSan, played });

describe("repGlyphsByPly", () => {
  it("returns an empty map for no marks", () => {
    expect(repGlyphsByPly([]).size).toBe(0);
  });

  it("ticks every matched move with a color-aware tooltip", () => {
    const map = repGlyphsByPly([
      mark(1, "white", "e4", "matched"),
      mark(2, "black", "e5", "matched"),
    ]);
    expect(map.get(1)).toEqual({
      kind: "match",
      color: "white",
      expectedSan: "e4",
      title: "in your White repertoire",
    });
    expect(map.get(2)?.title).toBe("in your Black repertoire");
  });

  it("marks only the FIRST deviation per color, naming the expected move", () => {
    const map = repGlyphsByPly([
      mark(1, "white", "e4", "matched"),
      mark(3, "white", "Bb5", "deviated"),
      // Transposed back into book, then out again: no second deviation mark.
      mark(7, "white", "d4", "deviated"),
      mark(2, "black", "c5", "deviated"),
    ]);
    expect(map.get(3)).toEqual({
      kind: "deviation",
      color: "white",
      expectedSan: "Bb5",
      title: "your White repertoire plays Bb5 here",
    });
    expect(map.has(7)).toBe(false);
    // Each color gets its own first deviation.
    expect(map.get(2)?.kind).toBe("deviation");
    expect(map.get(2)?.title).toBe("your Black repertoire plays c5 here");
    expect(map.get(1)?.kind).toBe("match");
  });

  it("picks the first deviation by ply even when marks arrive unsorted", () => {
    const map = repGlyphsByPly([
      mark(9, "white", "h4", "deviated"),
      mark(5, "white", "Bb5", "deviated"),
    ]);
    expect(map.get(5)?.kind).toBe("deviation");
    expect(map.has(9)).toBe(false);
  });

  it("keeps matched ticks after a deviation (transposition back into book)", () => {
    const map = repGlyphsByPly([
      mark(3, "white", "Bb5", "deviated"),
      mark(7, "white", "d4", "matched"),
    ]);
    expect(map.get(7)?.kind).toBe("match");
  });
});
