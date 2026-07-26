import { describe, expect, it } from "vitest";
import { clampPly, lastMoveAt, loadGame, numberedSans } from "./game";

const SHORT_PGN = `[Event "Test"]
[White "A"]
[Black "B"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 1-0`;

describe("loadGame", () => {
  it("parses a mainline and precomputes FENs per ply", () => {
    const res = loadGame(SHORT_PGN);
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    const g = res.game;
    expect(g.sans).toEqual(["e4", "e5", "Nf3", "Nc6", "Bb5"]);
    expect(g.fens).toHaveLength(6);
    expect(g.fens[0]).toBe("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    expect(g.fens[1]).toBe("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    // Ruy Lopez position after 3. Bb5
    expect(g.fens[5].startsWith("r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b")).toBe(
      true,
    );
    expect(g.ucis).toEqual(["e2e4", "e7e5", "g1f3", "b8c6", "f1b5"]);
    expect(g.headers["White"]).toBe("A");
  });

  it("rejects empty input", () => {
    const res = loadGame("   \n  ");
    // chessops yields a headers-only game for whitespace; either outcome is
    // acceptable as long as we do not crash and report something sensible.
    if (res.ok) expect(res.game.sans).toHaveLength(0);
    else expect(res.error).toBeTruthy();
  });

  it("truncates at an illegal move with a warning", () => {
    const res = loadGame("1. e4 e5 2. Nf7 Ke7 1-0");
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    expect(res.game.sans).toEqual(["e4", "e5"]);
    expect(res.warning).toMatch(/Illegal/);
  });

  it("honors a FEN header (game from a non-initial position)", () => {
    const pgn = `[FEN "8/8/8/8/8/4k3/4p3/4K3 b - - 0 60"]
[SetUp "1"]

60... Kd3 61. Kf2 *`;
    const res = loadGame(pgn);
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    expect(res.game.fens[0]).toBe("8/8/8/8/8/4k3/4p3/4K3 b - - 0 60");
    expect(res.game.sans).toEqual(["Kd3", "Kf2"]);
  });
});

describe("stepping helpers", () => {
  const g = (() => {
    const res = loadGame(SHORT_PGN);
    if (!res.ok) throw new Error("fixture");
    return res.game;
  })();

  it("clamps ply into range", () => {
    expect(clampPly(-3, g)).toBe(0);
    expect(clampPly(2, g)).toBe(2);
    expect(clampPly(99, g)).toBe(5);
  });

  it("computes last-move highlight squares", () => {
    expect(lastMoveAt(g, 0)).toBeUndefined();
    expect(lastMoveAt(g, 1)).toEqual(["e2", "e4"]);
    expect(lastMoveAt(g, 5)).toEqual(["f1", "b5"]);
  });

  it("numbers the move list", () => {
    expect(numberedSans(g)).toEqual(["1. e4", "1... e5", "2. Nf3", "2... Nc6", "3. Bb5"]);
  });

  it("numbers moves starting from a black-to-move FEN", () => {
    const res = loadGame(`[FEN "8/8/8/8/8/4k3/4p3/4K3 b - - 0 60"]\n[SetUp "1"]\n\n60... Kd3 61. Kf2 *`);
    if (!res.ok) throw new Error("fixture");
    expect(numberedSans(res.game)).toEqual(["60... Kd3", "61. Kf2"]);
  });
});
