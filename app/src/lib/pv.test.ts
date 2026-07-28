import { describe, expect, it } from "vitest";
import { numberSanLine, replaySanLine, uciPvToSan, PV_DISPLAY_PLIES, PV_INSERT_PLIES } from "./pv";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const BLACK_TO_MOVE = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
// After 13...exf4 in a Vienna-ish position — White to move at move 14.
const MOVE_14 = "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 4 14";

describe("uciPvToSan", () => {
  it("converts a PV to SAN by replay", () => {
    expect(uciPvToSan(START, ["e2e4", "e7e5", "g1f3", "b8c6"])).toEqual([
      "e4",
      "e5",
      "Nf3",
      "Nc6",
    ]);
  });

  it("stops at the first illegal move (legal prefix)", () => {
    expect(uciPvToSan(START, ["e2e4", "e2e4"])).toEqual(["e4"]);
    expect(uciPvToSan(START, ["zz99"])).toEqual([]);
  });

  it("handles promotions", () => {
    const fen = "8/P7/8/8/8/8/7k/K7 w - - 0 50";
    expect(uciPvToSan(fen, ["a7a8q"])).toEqual(["a8=Q"]);
  });

  it("handles castling in the engine's e1g1 encoding", () => {
    const fen = "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 6 5";
    expect(uciPvToSan(fen, ["e1g1", "e8g8"])).toEqual(["O-O", "O-O"]);
  });

  it("returns empty on a bad FEN", () => {
    expect(uciPvToSan("not a fen", ["e2e4"])).toEqual([]);
  });
});

describe("numberSanLine", () => {
  it("numbers a white-to-move line compactly", () => {
    expect(numberSanLine(MOVE_14, ["Qg3", "dxe5", "fxe5", "Nh5"])).toBe(
      "14.Qg3 dxe5 15.fxe5 Nh5",
    );
  });

  it("numbers a black-to-move line with the ... prefix once", () => {
    expect(numberSanLine(BLACK_TO_MOVE, ["e5", "Nf3", "Nc6"])).toBe("1...e5 2.Nf3 Nc6");
  });

  it("caps at maxPlies with an ellipsis", () => {
    expect(numberSanLine(START, ["e4", "e5", "Nf3", "Nc6"], 3)).toBe("1.e4 e5 2.Nf3 …");
    // No ellipsis when the whole line fits.
    expect(numberSanLine(START, ["e4", "e5"], 8)).toBe("1.e4 e5");
  });

  it("caps are sane: display below insert", () => {
    expect(PV_DISPLAY_PLIES).toBeLessThanOrEqual(PV_INSERT_PLIES);
  });
});

describe("replaySanLine", () => {
  it("collects per-ply FENs and UCIs", () => {
    const r = replaySanLine(START, ["e4", "e5"]);
    expect(r.sans).toEqual(["e4", "e5"]);
    expect(r.ucis).toEqual(["e2e4", "e7e5"]);
    expect(r.fens).toHaveLength(3);
    expect(r.fens[0].split(" ")[0]).toBe("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR");
    expect(r.fens[1]).toContain("4P3");
    expect(r.fens[2].split(" ")[1]).toBe("w");
  });

  it("stops at the first illegal SAN (legal prefix)", () => {
    const r = replaySanLine(START, ["e4", "Ke2", "e5"]);
    expect(r.sans).toEqual(["e4"]);
    expect(r.fens).toHaveLength(2);
  });

  it("plays null moves ('--') as a side-to-move flip with null uci", () => {
    const r = replaySanLine(START, ["e4", "--", "d4"]);
    expect(r.sans).toEqual(["e4", "--", "d4"]);
    expect(r.ucis).toEqual(["e2e4", null, "d2d4"]);
    expect(r.fens[2].split(" ")[1]).toBe("w"); // flip: white again
  });

  it("returns empty arrays on a bad FEN", () => {
    expect(replaySanLine("junk", ["e4"])).toEqual({ fens: [], sans: [], ucis: [] });
  });
});
