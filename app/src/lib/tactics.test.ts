import { describe, expect, it } from "vitest";
import { buildPuzzleModel, formatClock, isSolverMove, motifWeightsFromProfile } from "./tactics";
import type { PlayerProfile } from "./db";

describe("buildPuzzleModel", () => {
  // Fixture puzzle gEacS (testdata/fixtures/puzzles_sample.csv): Black to
  // solve after White's setup move 31.Ng5 (f3g5 is BLACK's knight? no —
  // the FEN has Black to move, so moves[0] f3g5 is Black's setup move and
  // the USER solves as White).
  const fen = "6k1/p3rpp1/2p2r2/8/4p1q1/P1N1PnP1/1P2RPK1/3Q3R b - - 1 30";
  const moves = ["f3g5", "d1d8", "e7e8", "d8e8"];

  it("plays the full line and derives the solver color", () => {
    const m = buildPuzzleModel(fen, moves);
    expect(m).not.toBeNull();
    expect(m!.fens).toHaveLength(5);
    expect(m!.sans).toEqual(["Ng5", "Qd8+", "Re8", "Qxe8#"]);
    expect(m!.solverColor).toBe("white");
    expect(m!.lastMoves[0]).toEqual(["f3", "g5"]);
    // Final position is checkmate: the model must reach it legally.
    expect(m!.fens[4]).toContain("4Q1k1");
  });

  it("rejects corrupt lines and bad FENs", () => {
    expect(buildPuzzleModel(fen, ["a1a2"])).toBeNull(); // illegal
    expect(buildPuzzleModel(fen, ["zz"])).toBeNull(); // unparseable
    expect(buildPuzzleModel("not a fen", moves)).toBeNull();
  });

  it("handles castling UCI in stored lines", () => {
    const m = buildPuzzleModel("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", ["e1g1", "e8c8"]);
    expect(m).not.toBeNull();
    expect(m!.sans).toEqual(["O-O", "O-O-O"]);
    expect(m!.solverColor).toBe("black");
  });
});

describe("solve-flow helpers", () => {
  it("odd line indices are the user's moves", () => {
    expect(isSolverMove(0)).toBe(false); // setup move
    expect(isSolverMove(1)).toBe(true);
    expect(isSolverMove(2)).toBe(false);
    expect(isSolverMove(3)).toBe(true);
  });

  it("formats drill clocks", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(59_400)).toBe("0:59");
    expect(formatClock(185_000)).toBe("3:05");
  });

  it("extracts motif weights from a profile", () => {
    const profile = {
      motifs: [
        {
          kind: "Undefended",
          opportunities: 10,
          taken: 4,
          missed: 6,
          allowed: 1318,
          example_missed: [],
          example_allowed: [],
        },
      ],
    } as unknown as PlayerProfile;
    expect(motifWeightsFromProfile(profile)).toEqual([
      { kind: "Undefended", allowed: 1318, missed: 6 },
    ]);
  });
});
