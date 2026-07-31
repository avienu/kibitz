import { describe, expect, it } from "vitest";
import {
  defaultTriageColor,
  evalLabel,
  inferredLineLabel,
  itemCaption,
  numberedLine,
  triageSummary,
  type ColorTriage,
  type InferredLine,
  type TriageItem,
  type TriageReport,
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
    gamesSeen: 12,
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

  it("never says '0 games' for a skipped color — it says why", () => {
    expect(triageSummary({ ...base, hasCards: false, gamesScanned: 0 })).toBe(
      "White games are skipped until a White repertoire exists — adopt one below.",
    );
    expect(
      triageSummary({ ...base, color: "black", hasCards: false, gamesScanned: 0 }),
    ).toBe("Black games are skipped until a Black repertoire exists — adopt one below.");
  });
});

describe("defaultTriageColor", () => {
  const ct = (over: Partial<ColorTriage>): ColorTriage => ({
    color: "white",
    hasCards: false,
    gamesScanned: 0,
    gamesSeen: 0,
    deviations: [],
    gaps: [],
    frontiers: [],
    ...over,
  });
  const report = (white: Partial<ColorTriage>, black: Partial<ColorTriage>): TriageReport => ({
    player: "Infer, Ida",
    white: ct(white),
    black: ct({ color: "black", ...black }),
  });

  it("picks a color that has cards (White when both do)", () => {
    expect(defaultTriageColor(report({ hasCards: true }, {}))).toBe("white");
    expect(defaultTriageColor(report({}, { hasCards: true }))).toBe("black");
    expect(defaultTriageColor(report({ hasCards: true }, { hasCards: true }))).toBe("white");
  });

  it("with no cards anywhere, picks the color with more cohort games", () => {
    expect(defaultTriageColor(report({ gamesSeen: 2 }, { gamesSeen: 9 }))).toBe("black");
    expect(defaultTriageColor(report({ gamesSeen: 9 }, { gamesSeen: 2 }))).toBe("white");
    // Ties (including 0–0) fall back to White deterministically.
    expect(defaultTriageColor(report({}, {}))).toBe("white");
  });

  it("cards beat game counts — never a dead tab", () => {
    expect(defaultTriageColor(report({ hasCards: true, gamesSeen: 1 }, { gamesSeen: 99 }))).toBe(
      "white",
    );
  });
});

describe("inferredLineLabel", () => {
  const line = (over: Partial<InferredLine>): InferredLine => ({
    sans: ["e4", "c5", "Nf3"],
    games: 4,
    score: 62.5,
    eco: "B27",
    openingName: "Sicilian Defense",
    ...over,
  });

  it("joins games, score and the dataset name", () => {
    expect(inferredLineLabel(line({}))).toBe("4 games · 62.5% score · B27 Sicilian Defense");
    expect(inferredLineLabel(line({ games: 1, score: 100 }))).toBe(
      "1 game · 100% score · B27 Sicilian Defense",
    );
  });

  it("omits missing name parts instead of printing placeholders", () => {
    expect(inferredLineLabel(line({ eco: null }))).toBe("4 games · 62.5% score · Sicilian Defense");
    expect(inferredLineLabel(line({ eco: null, openingName: null }))).toBe(
      "4 games · 62.5% score",
    );
  });
});
