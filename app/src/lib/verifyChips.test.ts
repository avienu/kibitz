/**
 * CONSIDER-chip verification state machine (run 11): pending →
 * cleared/refuted transitions, the marked-hidden-until-cleared rule,
 * stale-drop by FEN stamp, and the engine-unavailable path.
 */
import { describe, expect, it } from "vitest";
import type { ExplanationJson, SuggestionJson } from "./gameView";
import {
  failVerification,
  needsVerification,
  resolveVerification,
  visibleChips,
  type VerificationState,
  type VerifyOut,
} from "./verifyChips";

const WINAWER = "rnbqk1nr/pp3ppp/4p3/2ppP3/1b1P4/P1N5/1PP2PPP/R1BQKBNR b KQkq - 0 5";

function sug(san: string, uci: string, staticRisk?: number): SuggestionJson {
  return {
    san,
    uci,
    score: 3,
    prophylactic: false,
    static_risk: staticRisk ?? null,
    evidence: {},
  };
}

/** The Winawer field report: all three shipped chips statically marked. */
function firedExplanation(suggestions: SuggestionJson[]): ExplanationJson {
  return {
    schema_version: 3,
    tag: "TACTICAL SCREEN FIRED",
    headline: { coach: "", neutral: "" },
    blocks: [],
    suggestions,
  };
}

const winawerSuggestions = [
  sug("cxd4", "c5d4", 230),
  sug("f5", "f7f5", 230),
  sug("f6", "f7f6", 230),
];

describe("needsVerification", () => {
  it("fires only for a fired screen with suggestions", () => {
    expect(needsVerification(firedExplanation(winawerSuggestions))).toBe(true);
    expect(needsVerification(firedExplanation([]))).toBe(false);
    expect(
      needsVerification({ ...firedExplanation(winawerSuggestions), tag: "QUIET POSITION" }),
    ).toBe(false);
    expect(needsVerification(null)).toBe(false);
  });
});

describe("visibleChips", () => {
  it("hides statically-marked chips until cleared; clean chips show", () => {
    const mixed = firedExplanation([sug("Nd5", "c3d5"), sug("f5", "f7f5", 230)]);
    // No verification yet: marked hidden, clean visible, no pending.
    const idle = visibleChips(mixed, null);
    expect(idle.map((c) => c.s.san)).toEqual(["Nd5"]);
    expect(idle[0].pending).toBe(false);
    // Winawer: everything marked → NOTHING renders statically.
    expect(visibleChips(firedExplanation(winawerSuggestions), null)).toEqual([]);
  });

  it("marks clean chips pending while the round-trip runs", () => {
    const mixed = firedExplanation([sug("Nd5", "c3d5"), sug("f5", "f7f5", 230)]);
    const running: VerificationState = { kind: "running", fen: WINAWER };
    const chips = visibleChips(mixed, running);
    expect(chips.map((c) => c.s.san)).toEqual(["Nd5"]);
    expect(chips[0].pending).toBe(true);
  });

  it("applies verdicts: refuted chips disappear, cleared marked chips appear", () => {
    const done: VerificationState = {
      kind: "done",
      fen: WINAWER,
      verdicts: { c5d4: "cleared", f7f5: "refuted", f7f6: "refuted" },
    };
    const chips = visibleChips(firedExplanation(winawerSuggestions), done);
    // The Winawer outcome: only the theory move cxd4 survives.
    expect(chips.map((c) => c.s.san)).toEqual(["cxd4"]);
    // Its hover index still addresses the ORIGINAL suggestion slot.
    expect(chips[0].index).toBe(0);
    expect(chips[0].pending).toBe(false);
  });

  it("keeps original hover indices for later slots", () => {
    const done: VerificationState = {
      kind: "done",
      fen: WINAWER,
      verdicts: { c5d4: "refuted", f7f5: "refuted", f7f6: "cleared" },
    };
    const chips = visibleChips(firedExplanation(winawerSuggestions), done);
    expect(chips).toHaveLength(1);
    expect(chips[0].index).toBe(2);
  });

  it("engine unavailable: marked chips stay hidden, pending clears", () => {
    const mixed = firedExplanation([sug("Nd5", "c3d5"), sug("f5", "f7f5", 230)]);
    const chips = visibleChips(mixed, { kind: "unavailable" });
    expect(chips.map((c) => c.s.san)).toEqual(["Nd5"]);
    expect(chips[0].pending).toBe(false);
  });
});

describe("resolveVerification / failVerification", () => {
  const running: VerificationState = { kind: "running", fen: WINAWER };
  const result: VerifyOut = {
    fen: WINAWER,
    ran: true,
    verdicts: [
      { uci: "c5d4", san: "cxd4", verdict: "cleared" },
      { uci: "f7f5", san: "f5", verdict: "refuted" },
    ],
    nodesPerSearch: 150000,
  };

  it("folds a matching result into done", () => {
    const next = resolveVerification(running, result);
    expect(next).toEqual({
      kind: "done",
      fen: WINAWER,
      verdicts: { c5d4: "cleared", f7f5: "refuted" },
    });
  });

  it("drops a stale result whose FEN stamp differs", () => {
    const stale = { ...result, fen: "8/8/8/8/8/8/8/K1k5 w - - 0 1" };
    expect(resolveVerification(running, stale)).toBe(running);
    // A settled state never regresses either.
    const done = resolveVerification(running, result);
    expect(resolveVerification(done, result)).toBe(done);
  });

  it("ran:false (server-side gate declined) settles as unavailable", () => {
    const next = resolveVerification(running, { ...result, ran: false, verdicts: [] });
    expect(next).toEqual({ kind: "unavailable" });
  });

  it("failure settles as unavailable only for the matching request", () => {
    expect(failVerification(running, WINAWER)).toEqual({ kind: "unavailable" });
    expect(failVerification(running, "some other fen")).toBe(running);
    const done: VerificationState = { kind: "done", fen: WINAWER, verdicts: {} };
    expect(failVerification(done, WINAWER)).toBe(done);
  });
});
