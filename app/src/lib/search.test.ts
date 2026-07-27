import { describe, expect, it } from "vitest";
import { castlingFromPlacement, fenFromPlacement, placementOf, turnOf } from "./search";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

describe("castlingFromPlacement", () => {
  it("grants full rights on the start position", () => {
    expect(castlingFromPlacement(START)).toBe("KQkq");
  });

  it("drops rights as kings/rooks leave their home squares", () => {
    // White king moved off e1: no white rights.
    expect(castlingFromPlacement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ1BNR")).toBe("kq");
    // White a-rook gone: queenside right lost, kingside kept.
    expect(castlingFromPlacement("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/1NBQKBNR")).toBe("Kkq");
    // Empty board: none.
    expect(castlingFromPlacement("8/8/8/8/8/8/8/8")).toBe("-");
  });
});

describe("fenFromPlacement", () => {
  it("assembles a searchable FEN with the chosen side to move", () => {
    expect(fenFromPlacement(START, "white")).toBe(`${START} w KQkq - 0 1`);
    expect(fenFromPlacement("8/8/8/8/8/8/8/8", "black")).toBe("8/8/8/8/8/8/8/8 b - - 0 1");
  });
});

describe("placementOf / turnOf", () => {
  it("splits a full FEN", () => {
    expect(placementOf(`${START} w KQkq - 0 1`)).toBe(START);
    expect(turnOf(`${START} b KQkq - 0 1`)).toBe("black");
    expect(turnOf(`${START} w KQkq - 0 1`)).toBe("white");
  });
});
