import { describe, expect, it } from "vitest";
import { fenTurn } from "./fen";

describe("fenTurn", () => {
  it("reads the side to move", () => {
    expect(fenTurn("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")).toBe("white");
    // Lichess puzzle R66Pz, the audit reproduction: Black to move must NOT
    // leave chessground on its default turnColor ("white"), or the user's
    // moves become silently-queued premoves.
    expect(fenTurn("r2bk2r/p4p1p/2pp4/6n1/3q4/2N4P/PP1B2P1/R3R1K1 b - - 0 1")).toBe("black");
  });

  it("defaults to white on malformed input", () => {
    expect(fenTurn("garbage")).toBe("white");
    expect(fenTurn("")).toBe("white");
  });
});
