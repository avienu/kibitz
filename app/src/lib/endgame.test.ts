import { describe, expect, it } from "vitest";
import { destsFor, goalText, lastMoveOf, masteryLabel, uciForDrag } from "./endgame";

// Lucena drill position (curriculum id "lucena").
const LUCENA = "1K6/1P1k4/8/8/8/8/r7/2R5 w - - 0 1";
// Square-of-the-pawn defense (curriculum id "square-rule-hold").
const SQUARE_HOLD = "8/8/8/8/P3k3/8/8/K7 b - - 0 1";

describe("uciForDrag", () => {
  it("accepts a legal rook lift and rejects an illegal one", () => {
    expect(uciForDrag(LUCENA, "c1", "c4")).toBe("c1c4");
    expect(uciForDrag(LUCENA, "c1", "d2")).toBeNull(); // rooks don't move diagonally
    expect(uciForDrag(LUCENA, "b7", "b8")).toBeNull(); // own king sits on b8
  });

  it("promotes to a queen by default", () => {
    // White king clear of the pawn's path: b7-b8 promotes.
    expect(uciForDrag("8/1P1k4/1K6/8/8/8/r7/2R5 w - - 0 1", "b7", "b8")).toBe("b7b8q");
  });

  it("returns null on garbage input", () => {
    expect(uciForDrag("not a fen", "e2", "e4")).toBeNull();
    expect(uciForDrag(LUCENA, "zz", "e4")).toBeNull();
  });
});

describe("destsFor", () => {
  it("maps only the side to move", () => {
    const dests = destsFor(SQUARE_HOLD, "black");
    expect(dests.get("e4")).toContain("d5");
    expect(destsFor(SQUARE_HOLD, "white").size).toBe(0);
  });
});

describe("labels", () => {
  it("renders the task line for both goals", () => {
    expect(goalText("win", "white")).toBe("Win with White");
    expect(goalText("draw", "black")).toBe("Hold the draw with Black");
  });

  it("renders mastery progress", () => {
    expect(masteryLabel(0, 2, false)).toBe("");
    expect(masteryLabel(1, 2, false)).toBe("1/2 clean");
    expect(masteryLabel(2, 2, true)).toBe("mastered");
    expect(masteryLabel(0, 2, true)).toBe("mastered"); // mastery persists
  });

  it("extracts last-move highlights", () => {
    expect(lastMoveOf("e7e8q")).toEqual(["e7", "e8"]);
    expect(lastMoveOf("bad")).toBeUndefined();
  });
});
