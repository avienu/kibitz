import { describe, expect, it } from "vitest";
import { liveInitial, liveReduce, type LiveState } from "./liveAnalysis";

const FEN1 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
const FEN2 = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";

describe("live-analysis controller", () => {
  it("starts off and never on by default", () => {
    expect(liveInitial.on).toBe(false);
    expect(liveInitial.searching).toBeNull();
  });

  it("toggle on starts an infinite search on the current fen", () => {
    const { next, commands } = liveReduce(liveInitial, { type: "toggle", fen: FEN1 });
    expect(next).toEqual({ on: true, searching: FEN1 });
    expect(commands).toEqual([{ kind: "start", fen: FEN1 }]);
  });

  it("toggle off hard-stops", () => {
    const on: LiveState = { on: true, searching: FEN1 };
    const { next, commands } = liveReduce(on, { type: "toggle", fen: FEN1 });
    expect(next).toEqual({ on: false, searching: null });
    expect(commands).toEqual([{ kind: "stop" }]);
  });

  it("position change while on restarts on the new fen", () => {
    const on: LiveState = { on: true, searching: FEN1 };
    const { next, commands } = liveReduce(on, { type: "fenChanged", fen: FEN2 });
    expect(next).toEqual({ on: true, searching: FEN2 });
    expect(commands).toEqual([{ kind: "stop" }, { kind: "start", fen: FEN2 }]);
  });

  it("same fen while on is a no-op; any fen while off is a no-op", () => {
    const on: LiveState = { on: true, searching: FEN1 };
    expect(liveReduce(on, { type: "fenChanged", fen: FEN1 }).commands).toEqual([]);
    expect(liveReduce(liveInitial, { type: "fenChanged", fen: FEN2 }).commands).toEqual([]);
  });

  it("leaving the game view hard-stops and resets", () => {
    const on: LiveState = { on: true, searching: FEN1 };
    const { next, commands } = liveReduce(on, { type: "leave" });
    expect(next).toEqual({ on: false, searching: null });
    expect(commands).toEqual([{ kind: "stop" }]);
    // Idempotent when already off.
    expect(liveReduce(liveInitial, { type: "leave" }).commands).toEqual([]);
  });
});
