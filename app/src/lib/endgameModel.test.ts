import { describe, expect, it } from "vitest";
import type { DrillProgress, MoveResponse, StartedDrill, VerdictRow } from "./endgame";
import {
  applyGiveUp,
  applyMoveResponse,
  beginDrill,
  canMove,
  commitReply,
  failureReason,
  isTerminal,
  nextDrillId,
  progressNote,
  statusLine,
} from "./endgameModel";

/** KQ-mate drill fixture (the field-report scenario). */
const STARTED: StartedDrill = {
  drillId: "kq-mate",
  title: "Queen mate",
  instruction: "Box the king to the edge, then bring your own king up.",
  goal: "win",
  fen: "3k4/8/3K4/2Q5/8/8/8/8 w - - 0 1",
  userSide: "white",
  opponentTablebase: true,
};

const row = (index: number, san: string, verdict: VerdictRow["verdict"], note = ""): VerdictRow => ({
  index,
  san,
  verdict,
  note,
});

const PROGRESS: DrillProgress = {
  drillId: "kq-mate",
  attempts: 3,
  solved: 2,
  cleanStreak: 1,
  mastered: false,
};

/** A mid-drill step: user move graded, defender replies, drill continues. */
const STEP_CONTINUES: MoveResponse = {
  fenAfterUser: "3k4/8/3K4/8/8/8/8/2Q5 w - - 1 1",
  opponent: { uci: "d8e8", source: "tablebase" },
  fenAfterOpponent: "4k3/8/3K4/2Q5/8/8/8/8 w - - 2 2",
  rows: [row(1, "Qc5+", "winning"), row(2, "Ke8", "engine")],
  outcome: null,
  progress: null,
};

describe("endgame drill state machine (userTurn → replying → userTurn → terminal)", () => {
  it("starts on the user's turn with an empty session", () => {
    const m = beginDrill(STARTED);
    expect(m.phase).toBe("userTurn");
    expect(canMove(m)).toBe(true);
    expect(m.fen).toBe(STARTED.fen);
    expect(m.rows).toHaveLength(0);
    expect(statusLine(m)).toEqual({ tone: "play", text: "Your move — win with White." });
  });

  it("a continuing move parks the reply, then commit returns the turn to the user", () => {
    const m1 = applyMoveResponse(beginDrill(STARTED), "c5c5", STEP_CONTINUES);
    // Mid-beat: the user's move is on the board, input is off, status says so.
    expect(m1.phase).toBe("replying");
    expect(canMove(m1)).toBe(false);
    expect(m1.fen).toBe(STEP_CONTINUES.fenAfterUser);
    expect(m1.rows.map((r) => r.verdict)).toEqual(["winning", "engine"]);
    expect(statusLine(m1)).toEqual({ tone: "wait", text: "Defender is thinking…" });
    // The beat lands: defender's move on the board, USER CAN MOVE AGAIN.
    const m2 = commitReply(m1);
    expect(m2.phase).toBe("userTurn");
    expect(canMove(m2)).toBe(true);
    expect(m2.fen).toBe(STEP_CONTINUES.fenAfterOpponent);
    expect(m2.lastMove).toEqual(["d8", "e8"]);
    expect(m2.pendingReply).toBeNull();
    // The loop repeats: a second continuing move parks a second reply.
    const m3 = applyMoveResponse(m2, "c5c7", STEP_CONTINUES);
    expect(m3.phase).toBe("replying");
    expect(m3.rows).toHaveLength(4);
    expect(m3.userMoves).toBe(2);
  });

  it("delivering mate (no reply) is an immediate solved terminal", () => {
    const mate: MoveResponse = {
      fenAfterUser: "4k1Q1/8/4K3/8/8/8/8/8 b - - 5 3",
      opponent: null,
      fenAfterOpponent: null,
      rows: [row(1, "Qg8#", "winning", "Checkmate!")],
      outcome: { solved: true, detail: "Checkmate!" },
      progress: PROGRESS,
    };
    const m = applyMoveResponse(beginDrill(STARTED), "c5g8", mate);
    expect(m.phase).toBe("solved");
    expect(isTerminal(m)).toBe(true);
    expect(canMove(m)).toBe(false);
    expect(statusLine(m)).toEqual({ tone: "good", text: "Solved — Checkmate!" });
    expect(progressNote(m, 2)).toBe("Clean streak 1/2.");
  });

  it("a tablebase result-flip fails immediately and names the losing move", () => {
    const throws: MoveResponse = {
      fenAfterUser: "3k4/2Q5/3K4/8/8/8/8/8 b - - 1 1",
      opponent: null,
      fenAfterOpponent: null,
      rows: [row(1, "Qc7+??", "throws", "Throws away the win: the position is now drawn.")],
      outcome: { solved: false, detail: "That move throws away the win." },
      progress: { ...PROGRESS, cleanStreak: 0 },
    };
    const m = applyMoveResponse(beginDrill(STARTED), "c5c7", throws);
    expect(m.phase).toBe("failed");
    expect(statusLine(m).tone).toBe("bad");
    expect(failureReason(m)).toBe(
      "Qc7+?? — Throws away the win: the position is now drawn.",
    );
  });

  it("a reply that ends the drill terminates on commit, not before", () => {
    const stalemated: MoveResponse = {
      ...STEP_CONTINUES,
      rows: [row(1, "Qb6", "throws", "Stalemate."), row(2, "Kd8", "engine")],
      outcome: { solved: false, detail: "Only a draw (stalemate) — the position was winning." },
      progress: { ...PROGRESS, cleanStreak: 0 },
    };
    const m1 = applyMoveResponse(beginDrill(STARTED), "c5b6", stalemated);
    expect(m1.phase).toBe("replying"); // outcome rides the pending reply
    const m2 = commitReply(m1);
    expect(m2.phase).toBe("failed");
    expect(m2.outcome?.detail).toContain("stalemate");
    expect(m2.progress?.cleanStreak).toBe(0);
  });

  it("give up fails the drill from either live phase; commit is a no-op after", () => {
    const live = applyMoveResponse(beginDrill(STARTED), "c5c5", STEP_CONTINUES);
    const conceded = applyGiveUp(commitReply(live), { ...PROGRESS, cleanStreak: 0 });
    expect(conceded.phase).toBe("failed");
    expect(conceded.outcome?.detail).toBe("Gave up.");
    expect(failureReason(conceded)).toBeNull(); // no THROWS row — nothing to blame
    expect(commitReply(conceded)).toBe(conceded);
    // Terminal states cannot be conceded again.
    expect(applyGiveUp(conceded, PROGRESS)).toBe(conceded);
  });
});

describe("next-drill affordance", () => {
  const order = [
    { id: "a", mastered: true },
    { id: "b", mastered: false },
    { id: "c", mastered: true },
    { id: "d", mastered: false },
  ];

  it("prefers the first unmastered drill after the current one, wrapping", () => {
    expect(nextDrillId(order, "a")).toBe("b");
    expect(nextDrillId(order, "b")).toBe("d");
    expect(nextDrillId(order, "d")).toBe("b"); // wraps past a and c
  });

  it("falls back to plain curriculum order when everything is mastered", () => {
    const all = order.map((d) => ({ ...d, mastered: true }));
    expect(nextDrillId(all, "b")).toBe("c");
    expect(nextDrillId(all, "d")).toBe("a");
  });

  it("returns null for unknown or solitary drills", () => {
    expect(nextDrillId(order, "zz")).toBeNull();
    expect(nextDrillId([{ id: "a", mastered: false }], "a")).toBeNull();
  });
});
