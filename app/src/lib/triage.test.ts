import { describe, expect, it } from "vitest";
import {
  evalLabel,
  itemCaption,
  numberedLine,
  triageSummary,
  type ColorTriage,
  type TriageItem,
} from "./triage";

/** After 1.e4 c5 — White to move, move 2 (the Sicilian gap spot). */
const W_FEN = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
/** After 1.e4 e5 2.Nf3 Nc6 3.Bb5 — Black to move, move 3 (frontier). */
const B_FEN = "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3";

describe("evalLabel (white-POV from stored side-to-move evals)", () => {
  it("passes white-to-move cp through and formats the sign", () => {
    expect(evalLabel({ sans: ["Nf3"], scoreCp: 35 }, W_FEN)).toBe("+0.35");
    expect(evalLabel({ sans: ["Nf3"], scoreCp: -110 }, W_FEN)).toBe("−1.10");
    expect(evalLabel({ sans: ["Nf3"], scoreCp: 0 }, W_FEN)).toBe("+0.00");
  });

  it("flips black-to-move evals to white POV", () => {
    // +40 for Black to move = −0.40 for White.
    expect(evalLabel({ sans: ["a6"], scoreCp: 40 }, B_FEN)).toBe("−0.40");
    expect(evalLabel({ sans: ["a6"], scoreCp: -25 }, B_FEN)).toBe("+0.25");
  });

  it("renders mates as # with the white-POV sign", () => {
    expect(evalLabel({ sans: ["Nf3"], scoreCp: 10_000, mate: 5 }, W_FEN)).toBe("#5");
    expect(evalLabel({ sans: ["a6"], scoreCp: 10_000, mate: 3 }, B_FEN)).toBe("#−3");
  });
});

describe("numberedLine", () => {
  it("numbers a white-to-move continuation", () => {
    expect(numberedLine(["Nf3", "d6", "d4", "cxd4"], W_FEN)).toBe("2. Nf3 d6 3. d4 cxd4");
  });

  it("starts black-to-move continuations with an ellipsis number", () => {
    expect(numberedLine(["a6", "Ba4", "Nf6"], B_FEN)).toBe("3... a6 4. Ba4 Nf6");
  });

  it("is empty for no moves", () => {
    expect(numberedLine([], W_FEN)).toBe("");
  });
});

function item(over: Partial<TriageItem>): TriageItem {
  return {
    fen: W_FEN,
    ply: 2,
    games: 1,
    line: "1. e4 c5",
    eco: null,
    openingName: null,
    expectedSan: null,
    playedSan: null,
    opponentSan: null,
    hasExtension: false,
    examples: [],
    ...over,
  };
}

describe("triageSummary and captions", () => {
  const base: ColorTriage = {
    color: "white",
    hasCards: true,
    gamesScanned: 12,
    deviations: [],
    gaps: [],
    frontiers: [],
  };

  it("names only non-zero classes, pluralized", () => {
    expect(
      triageSummary({ ...base, deviations: [item({})], gaps: [item({}), item({})] }),
    ).toBe("1 deviation · 2 gaps");
    expect(triageSummary({ ...base, frontiers: [item({})] })).toBe("1 frontier");
  });

  it("says so honestly when nothing was found", () => {
    expect(triageSummary(base)).toBe("no triage points in 12 games");
    expect(triageSummary({ ...base, gamesScanned: 1 })).toBe("no triage points in 1 game");
  });

  it("captions each class from its own fields", () => {
    expect(itemCaption("deviation", item({ expectedSan: "Bb5", playedSan: "Bc4" }))).toBe(
      "book: Bb5 — played Bc4",
    );
    expect(itemCaption("gap", item({ opponentSan: "c5" }))).toBe(
      "opponent played c5 — no card after it",
    );
    expect(itemCaption("frontier", item({}))).toBe("your book ends here");
  });
});
