import { describe, expect, it } from "vitest";
import { formatScore, pvToSan, summarizeInfo, type EngineInfo } from "./engineView";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const BLACK_TO_MOVE = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

describe("formatScore", () => {
  it("shows cp scores from white's POV", () => {
    expect(formatScore({ scoreCp: 53 }, START)).toBe("+0.53");
    expect(formatScore({ scoreCp: -120 }, START)).toBe("-1.20");
    // Black to move: +30 for the side to move is -0.30 for white.
    expect(formatScore({ scoreCp: 30 }, BLACK_TO_MOVE)).toBe("-0.30");
  });

  it("shows mate scores", () => {
    expect(formatScore({ scoreMate: 5 }, START)).toBe("#5");
    expect(formatScore({ scoreMate: -3 }, START)).toBe("#-3");
    expect(formatScore({ scoreMate: 2 }, BLACK_TO_MOVE)).toBe("#-2");
  });

  it("falls back when no score present", () => {
    expect(formatScore({}, START)).toBe("…");
  });
});

describe("pvToSan", () => {
  it("converts a UCI pv to numbered SAN", () => {
    expect(pvToSan(START, ["e2e4", "e7e5", "g1f3", "b8c6"])).toBe("1. e4 e5 2. Nf3 Nc6");
  });

  it("starts numbering correctly for black to move", () => {
    expect(pvToSan(BLACK_TO_MOVE, ["e7e5", "g1f3"])).toBe("1... e5 2. Nf3");
  });

  it("stops at an illegal move", () => {
    expect(pvToSan(START, ["e2e4", "e2e4"])).toBe("1. e4");
  });

  it("handles promotions and checks", () => {
    const fen = "8/P4k2/8/8/8/8/5K2/8 w - - 0 50";
    expect(pvToSan(fen, ["a7a8q"])).toBe("50. a8=Q");
  });
});

describe("summarizeInfo", () => {
  it("builds a compact one-liner", () => {
    const info: EngineInfo = { depth: 24, scoreCp: 53, nodes: 12_300_000, nps: 2_500_000 };
    expect(summarizeInfo(info, START)).toBe("d24  +0.53  12.3 Mnodes  2.5 Mn/s");
  });
});
