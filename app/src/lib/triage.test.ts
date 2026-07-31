import { describe, expect, it } from "vitest";
import {
  answerConfirmCopy,
  answerLineSans,
  defaultTriageColor,
  evalLabel,
  inBookGaps,
  inferredLineLabel,
  itemCaption,
  lineSans,
  numberedLine,
  numberedSan,
  opponentMoveLabel,
  realityDeviations,
  realityHeadline,
  triageSummary,
  wholeGapLabel,
  wholeOpeningGaps,
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
    playedCount: 0,
    cardFollowed: 0,
    realityCheck: false,
    inferredLines: [],
    wholeOpening: false,
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

  it("names only non-zero classes in the new declared-vs-played shapes", () => {
    expect(
      triageSummary({
        ...base,
        deviations: [item({})],
        gaps: [
          item({ wholeOpening: true }),
          item({ wholeOpening: true }),
          item({}),
          item({}),
          item({}),
        ],
      }),
    ).toBe(
      "your play disagrees with your cards at 1 position · 2 whole-opening holes · 3 in-book gaps",
    );
    expect(triageSummary({ ...base, frontiers: [item({})] })).toBe("1 frontier");
    expect(
      triageSummary({
        ...base,
        deviations: [item({}), item({})],
        gaps: [item({ wholeOpening: true })],
      }),
    ).toBe("your play disagrees with your cards at 2 positions · 1 whole-opening hole");
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

/* ---- declared-vs-played helpers (2026-07-30 v2) ---- */

/** After 1.e4 — Black (the user) to move, move 1. */
const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
/** After 1.d4 — Black (the user) to move, move 1. */
const AFTER_D4 = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq - 0 1";

describe("lineSans and numberedSan", () => {
  it("strips move numbers exactly (SAN never starts with a digit)", () => {
    expect(lineSans("1. e4 c5 2. Nf3 d6")).toEqual(["e4", "c5", "Nf3", "d6"]);
    expect(lineSans("1... c5")).toEqual(["c5"]);
    expect(lineSans("1. d4")).toEqual(["d4"]);
    expect(lineSans("")).toEqual([]);
    expect(lineSans("8. O-O Bxa1")).toEqual(["O-O", "Bxa1"]);
  });

  it("numbers a single move from a FEN, both sides", () => {
    expect(numberedSan(W_FEN, "Nf3")).toBe("2. Nf3");
    expect(numberedSan(AFTER_E4, "e5")).toBe("1... e5");
    expect(numberedSan(B_FEN, "a6")).toBe("3... a6");
  });
});

describe("reality-check and whole-opening helpers", () => {
  const reality = item({
    fen: AFTER_E4,
    ply: 2,
    games: 119,
    line: "1. e4",
    expectedSan: "e5",
    playedSan: "c5",
    playedCount: 119,
    cardFollowed: 1,
    realityCheck: true,
  });

  it("selects reality deviations and splits gaps by whole-opening", () => {
    const ct: ColorTriage = {
      color: "black",
      hasCards: true,
      gamesScanned: 199,
      gamesSeen: 199,
      deviations: [reality, item({ fen: B_FEN, expectedSan: "a6", playedSan: "Nf6" })],
      gaps: [
        item({ fen: AFTER_D4, opponentSan: "d4", wholeOpening: true, ply: 1, line: "1. d4" }),
        item({ opponentSan: "Nc3", ply: 3 }),
      ],
      frontiers: [],
    };
    expect(realityDeviations(ct)).toEqual([reality]);
    expect(wholeOpeningGaps(ct).map((g) => g.opponentSan)).toEqual(["d4"]);
    expect(inBookGaps(ct).map((g) => g.opponentSan)).toEqual(["Nc3"]);
  });

  it("writes the honest reality headline from real counts", () => {
    expect(realityHeadline(reality)).toBe(
      "Your cards say 1... e5 — but you've played 1... c5 in 119 of 120 games. " +
        "That looks like your real repertoire.",
    );
  });

  it("labels a whole-opening hole with the opponent's numbered move", () => {
    const hole = item({
      fen: AFTER_D4,
      opponentSan: "d4",
      games: 63,
      ply: 1,
      line: "1. d4",
      wholeOpening: true,
    });
    expect(opponentMoveLabel(hole)).toBe("1. d4");
    expect(wholeGapLabel(hole)).toBe("No repertoire vs 1. d4 (63 games)");
    // A White user's hole: the position after 1.e4 c5 records 1...c5.
    const whiteHole = item({ opponentSan: "c5", wholeOpening: true, games: 1 });
    expect(opponentMoveLabel(whiteHole)).toBe("1... c5");
    expect(wholeGapLabel(whiteHole)).toBe("No repertoire vs 1... c5 (1 game)");
  });
});

describe("board-played answers (I know my answer)", () => {
  it("builds the adopted line from the item's path plus the move", () => {
    // Gap: the path already ends with the opponent's move.
    const hole = item({ fen: AFTER_D4, opponentSan: "d4", line: "1. d4" });
    expect(answerLineSans(hole, "Nf6")).toEqual(["d4", "Nf6"]);
    const midGap = item({ opponentSan: "Nc3", line: "1. e4 c5 2. Nc3" });
    expect(answerLineSans(midGap, "Nc6")).toEqual(["e4", "c5", "Nc3", "Nc6"]);
    // Deviation: the path ends BEFORE the user's move.
    const dev = item({ fen: AFTER_E4, line: "1. e4", expectedSan: "e5" });
    expect(answerLineSans(dev, "c6")).toEqual(["e4", "c6"]);
  });

  it("asks before writing, naming the move and the opponent move", () => {
    const hole = item({ fen: AFTER_D4, opponentSan: "d4", line: "1. d4" });
    expect(answerConfirmCopy(hole, "Nf6")).toBe(
      "Set 1... Nf6 as your repertoire answer to 1. d4?",
    );
    // A deviation has no opponent move to name — the copy stays honest.
    const dev = item({ fen: AFTER_E4, line: "1. e4", opponentSan: null });
    expect(answerConfirmCopy(dev, "c6")).toBe("Set 1... c6 as your repertoire answer?");
  });
});
