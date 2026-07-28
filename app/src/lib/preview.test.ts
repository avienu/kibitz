import { describe, expect, it } from "vitest";
import { enterPreview, previewFen, previewLastMove, stepPreview } from "./preview";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
// Mainline 1. e4 e5 — a variation (1... c5 2. Nf3) branches at ply 2,
// so its branch-point fen is the position after 1. e4.
const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";

const ROW = { branchPly: 2, sans: ["c5", "Nf3"], label: "1...c5", varStartIndex: 4 };

describe("enterPreview", () => {
  it("reconstructs the branch line: branch fen first, then each move", () => {
    const p = enterPreview(AFTER_E4, ROW);
    expect(p).not.toBeNull();
    expect(p!.fens).toHaveLength(3);
    expect(p!.fens[0].split(" ")[0]).toBe(AFTER_E4.split(" ")[0]);
    expect(p!.fens[1]).toContain("2p5"); // ...c5 played
    expect(p!.sans).toEqual(["c5", "Nf3"]);
    expect(p!.branchPly).toBe(2);
    expect(p!.varStartIndex).toBe(4);
    expect(p!.label).toBe("1...c5");
  });

  it("opens ON the first variation move", () => {
    const p = enterPreview(AFTER_E4, ROW)!;
    expect(p.at).toBe(1);
    expect(previewFen(p)).toBe(p.fens[1]);
    expect(previewLastMove(p)).toEqual(["c7", "c5"]);
  });

  it("refuses empty or unreplayable variations", () => {
    expect(enterPreview(AFTER_E4, { branchPly: 2, sans: [], label: "" })).toBeNull();
    expect(enterPreview(AFTER_E4, { branchPly: 2, sans: ["Qxh8"], label: "x" })).toBeNull();
    expect(enterPreview("bad fen", ROW)).toBeNull();
  });

  it("previews the replayable prefix of a partially-illegal line", () => {
    const p = enterPreview(AFTER_E4, { branchPly: 2, sans: ["c5", "Qd5"], label: "1...c5" });
    expect(p!.sans).toEqual(["c5"]);
    expect(p!.fens).toHaveLength(2);
  });
});

describe("stepPreview", () => {
  const p = enterPreview(AFTER_E4, ROW)!;

  it("steps forward and back within the line", () => {
    const fwd = stepPreview(p, 1);
    expect(fwd.at).toBe(2);
    expect(previewFen(fwd)).toBe(p.fens[2]);
    expect(previewLastMove(fwd)).toEqual(["g1", "f3"]);
    const back = stepPreview(fwd, -1);
    expect(back.at).toBe(1);
  });

  it("clamps at both ends (stepping never exits)", () => {
    expect(stepPreview(stepPreview(p, 10), 1).at).toBe(2);
    const atStart = stepPreview(p, -10);
    expect(atStart.at).toBe(0);
    expect(previewFen(atStart)).toBe(p.fens[0]); // branch point shown
    expect(previewLastMove(atStart)).toBeUndefined();
  });

  it("returns the same object when clamped to the same position", () => {
    const end = stepPreview(p, 10);
    expect(stepPreview(end, 5)).toBe(end);
  });
});

describe("null moves in a previewed line", () => {
  it("flips the side to move and yields no highlight", () => {
    const p = enterPreview(START, { branchPly: 1, sans: ["--", "e5"], label: "--" })!;
    expect(p.sans).toEqual(["--", "e5"]);
    expect(previewLastMove(p)).toBeUndefined(); // at=1 is the null move
    const fwd = stepPreview(p, 1);
    expect(previewLastMove(fwd)).toEqual(["e7", "e5"]);
  });
});
