import { describe, expect, it } from "vitest";
import { PROFILE_FIXTURE } from "./profileFixture";
import {
  MODE_DEFS,
  isTimedMode,
  modeBadge,
  motifFact,
  seedMotifFromClaim,
  seededWeights,
  solvePrompt,
  sourceFact,
  tacticsKeyAction,
  weaknessLocked,
  weaknessWeights,
  whyText,
} from "./tacticsView";
import type { ServedPuzzle, TacticsState } from "./tactics";

const ST: TacticsState = { rating: 1842, attempts: 31, puzzles: 5000, themes: [{ theme: "fork", puzzles: 800 }] };

const SERVED: ServedPuzzle = {
  puzzle: {
    id: 1,
    lichessId: "abcde",
    fen: "8/8/8/8/8/8/8/8 w - - 0 1",
    moves: ["a1a2"],
    rating: 1795,
    popularity: 95,
    themes: ["fork", "middlegame"],
  },
  motif: "WeakKing",
  reason: "You allowed this 11 times.",
  matchedThemes: ["mate"],
  allowed: 11,
  missed: 6,
};

describe("seed contract (Train this weakness)", () => {
  it("parses motif claims and ignores everything else", () => {
    expect(seedMotifFromClaim("motif:WeakKing:missed")).toBe("WeakKing");
    expect(seedMotifFromClaim("motif:Undefended:allowed")).toBe("Undefended");
    expect(seedMotifFromClaim("structure:own-isolated-pawn")).toBeNull();
    expect(seedMotifFromClaim(null)).toBeNull();
    expect(seedMotifFromClaim(undefined)).toBeNull();
  });

  it("restricts the weakness weights to the seeded motif (the API's motif hint)", () => {
    expect(seededWeights(PROFILE_FIXTURE, "WeakKing")).toEqual([
      { kind: "WeakKing", allowed: 11, missed: 6 },
    ]);
    // No profile row: a synthetic unit weight still boosts the mapped themes.
    expect(seededWeights(null, "TrappedPiece")).toEqual([
      { kind: "TrappedPiece", allowed: 1, missed: 0 },
    ]);
  });

  it("unseeded weakness mode passes the whole profile through", () => {
    expect(weaknessWeights(PROFILE_FIXTURE, null)).toHaveLength(2);
    expect(weaknessWeights(null, null)).toBeUndefined();
    expect(weaknessWeights(PROFILE_FIXTURE, "WeakKing")).toHaveLength(1);
  });

  it("audit #15: weakness mode is locked only without BOTH profile and seed", () => {
    expect(weaknessLocked(null, null)).toBe(true);
    // A profile unlocks it; so does a "Train this weakness" seed alone.
    expect(weaknessLocked(PROFILE_FIXTURE, null)).toBe(false);
    expect(weaknessLocked(null, "WeakKing")).toBe(false);
    expect(weaknessLocked(PROFILE_FIXTURE, "WeakKing")).toBe(false);
  });
});

describe("modes", () => {
  it("weakness-targeted is the first (default) mode; clock only in timed modes", () => {
    expect(MODE_DEFS[0].id).toBe("weakness");
    expect(isTimedMode("speed")).toBe(true);
    expect(isTimedMode("woodpecker")).toBe(true);
    expect(isTimedMode("weakness")).toBe(false);
    expect(isTimedMode("rated")).toBe(false);
    expect(isTimedMode("motif")).toBe(false);
  });

  it("badges show real numbers or nothing — never invented", () => {
    expect(modeBadge("rated", ST, null, 0)).toBe("1842");
    expect(modeBadge("rated", { ...ST, attempts: 0 }, null, 0)).toBe("");
    expect(modeBadge("weakness", ST, PROFILE_FIXTURE, 0)).toBe("2 motifs");
    expect(modeBadge("weakness", ST, null, 0)).toBe("");
    expect(modeBadge("woodpecker", ST, null, 0)).toBe("");
    expect(modeBadge("woodpecker", ST, null, 2)).toBe("2 sets");
  });
});

describe("the reasoning aside", () => {
  it("weakness mode reuses the backend's per-pick reason as the body", () => {
    const coach = whyText("coach", "weakness", SERVED, {});
    expect(coach.body).toBe("You allowed this 11 times.");
    const neutral = whyText("neutral", "weakness", SERVED, {});
    expect(neutral.headline).toContain("allowed 11×");
    expect(neutral.headline).toContain("missed 6×");
  });

  it("facts are honest and never spoil the solution pre-solve", () => {
    expect(sourceFact("weakness", SERVED)).toBe("Your profile · allowed 11× · missed 6×");
    expect(sourceFact("rated", SERVED)).toBe("Lichess puzzle #abcde (CC0)");
    // Rated mode, unsolved: themes are hidden until finished.
    expect(motifFact("rated", SERVED, false)).toBe("revealed when solved");
    expect(motifFact("rated", SERVED, true)).toBe("fork, middlegame");
    // Weakness mode names the profiled motif (that is its whole point).
    expect(motifFact("weakness", SERVED, false)).toBe("Exposed king");
  });
});

describe("solve-flow prompt (no dead-ends: every state names its next action)", () => {
  it("walks a multi-move line: setup → your move (2) → correct/reply → keep going (1)", () => {
    // 4-move line: setup, user, reply, user (the tactics.test fixture).
    expect(solvePrompt("solving", 4, 0)).toEqual({
      tone: "wait",
      text: "Watch — the opponent plays first…",
    });
    expect(solvePrompt("solving", 4, 1)).toEqual({
      tone: "play",
      text: "Your move — 2 to find.",
    });
    expect(solvePrompt("solving", 4, 2)).toEqual({
      tone: "good",
      text: "✓ Correct — the opponent replies…",
    });
    expect(solvePrompt("solving", 4, 3)).toEqual({
      tone: "play",
      text: "✓ Keep going — one to find.",
    });
  });

  it("single-move puzzles prompt without a count", () => {
    expect(solvePrompt("solving", 2, 1)).toEqual({
      tone: "play",
      text: "Your move — find the best move.",
    });
  });

  it("terminal and idle phases yield to the outcome banner", () => {
    expect(solvePrompt("solved", 4, 4)).toBeNull();
    expect(solvePrompt("failed", 4, 1)).toBeNull();
    expect(solvePrompt("idle", 4, 0)).toBeNull();
  });
});

describe("keyboard map", () => {
  it("H / S / G / ⏎ / N map to actions", () => {
    expect(tacticsKeyAction("h", false)).toBe("hint");
    expect(tacticsKeyAction("H", false)).toBe("hint");
    expect(tacticsKeyAction("s", false)).toBe("skip");
    expect(tacticsKeyAction("g", false)).toBe("giveup");
    expect(tacticsKeyAction("Enter", false)).toBe("next");
    expect(tacticsKeyAction("n", false)).toBe("next");
    expect(tacticsKeyAction("N", false)).toBe("next");
    expect(tacticsKeyAction("x", false)).toBeNull();
  });

  it("focused inputs swallow every key (the editable exception)", () => {
    for (const key of ["h", "s", "g", "n", "Enter"]) {
      expect(tacticsKeyAction(key, true)).toBeNull();
    }
  });
});
