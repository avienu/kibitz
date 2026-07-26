import { describe, expect, it } from "vitest";
import {
  emptySummary,
  expectedArrow,
  formatInterval,
  sanForBoardMove,
  sanMatches,
  tallyAnswer,
  trainDests,
  turnOf,
} from "./train";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
/** After 1.e4 e5 2.Nf3 Nc6 3.Bc4 Bc5 — both sides may castle kingside. */
const ITALIAN = "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";

describe("sanForBoardMove", () => {
  it("names simple moves", () => {
    expect(sanForBoardMove(START, "e2", "e4")).toBe("e4");
    expect(sanForBoardMove(START, "g1", "f3")).toBe("Nf3");
  });
  it("rejects illegal moves and bad squares", () => {
    expect(sanForBoardMove(START, "e2", "e5")).toBeNull();
    expect(sanForBoardMove(START, "zz", "e4")).toBeNull();
    expect(sanForBoardMove("not a fen", "e2", "e4")).toBeNull();
  });
  it("normalizes both castling input forms", () => {
    expect(sanForBoardMove(ITALIAN, "e1", "g1")).toBe("O-O");
    expect(sanForBoardMove(ITALIAN, "e1", "h1")).toBe("O-O");
  });
});

describe("sanMatches", () => {
  it("ignores check, mate and annotation glyphs", () => {
    expect(sanMatches("Nf3", "Nf3")).toBe(true);
    expect(sanMatches("Qh4+", "Qh4")).toBe(true);
    expect(sanMatches("e4!?", "e4")).toBe(true);
    expect(sanMatches("Nf3", "Nc3")).toBe(false);
  });
});

describe("expectedArrow", () => {
  it("points from origin to destination", () => {
    expect(expectedArrow(START, "e4")).toEqual({ orig: "e2", dest: "e4" });
    expect(expectedArrow(START, "Nf3")).toEqual({ orig: "g1", dest: "f3" });
  });
  it("points castling at the king destination", () => {
    expect(expectedArrow(ITALIAN, "O-O")).toEqual({ orig: "e1", dest: "g1" });
  });
  it("is null for illegal SAN", () => {
    expect(expectedArrow(START, "Qh5")).toBeNull();
  });
});

describe("board wiring", () => {
  it("reports the side to move", () => {
    expect(turnOf(START)).toBe("white");
    expect(turnOf(ITALIAN.replace(" w ", " b "))).toBe("black");
    expect(turnOf("garbage")).toBeNull();
  });
  it("lists dests for the side to move", () => {
    const dests = trainDests(START);
    expect(dests.get("e2")).toContain("e4");
    expect(trainDests("garbage").size).toBe(0);
  });
});

describe("formatInterval", () => {
  it("rounds by magnitude", () => {
    expect(formatInterval(0.49)).toBe("<1d");
    expect(formatInterval(3.7145)).toBe("4d");
    expect(formatInterval(44)).toBe("44d");
    expect(formatInterval(90)).toBe("3mo");
    expect(formatInterval(548)).toBe("1.5y");
  });
});

describe("session summary", () => {
  it("tallies answers", () => {
    let s = emptySummary();
    s = tallyAnswer(s, true);
    s = tallyAnswer(s, false);
    s = tallyAnswer(s, true);
    expect(s).toEqual({ reviewed: 3, correct: 2, again: 1 });
  });
});
