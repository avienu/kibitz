import { describe, expect, it } from "vitest";
import type { BookExit, FingerprintRow, WeakLine } from "./db";
import { PROFILE_FIXTURE } from "./profileFixture";
import {
  bookExitFor,
  fingerprintRowWeak,
  lineName,
  lineScore,
  lineWhy,
  prepFinding,
  prepTimestamp,
  recordPrep,
  stepperSteps,
} from "./prepView";

const LINE: WeakLine = {
  hash: "123",
  eco: "B22",
  openingName: "Sicilian, Alapin",
  ply: 5,
  opponentMoves: ["Nc6"],
  games: 21,
  scorePct: 38,
  weakness: 2.5,
  deviation: true,
  masterGames: [],
};

describe("stepper values", () => {
  it("passed steps show their chosen values; unreached steps stay empty", () => {
    const v = { opponent: "R. Halvorsen", color: "black" as const, lineName: "Alapin", masterCount: 5 };
    expect(stepperSteps(v, 4).map((s) => s.value)).toEqual([
      "R. Halvorsen",
      "as Black",
      "Alapin",
      "5 games",
    ]);
    expect(stepperSteps(v, 2).map((s) => s.value)).toEqual(["R. Halvorsen", "as Black", null, null]);
    expect(stepperSteps({ ...v, opponent: null }, 1).map((s) => s.value)).toEqual([
      null,
      null,
      null,
      null,
    ]);
  });
});

describe("weak-line prose", () => {
  it("cites real counts and names the book-exit fact", () => {
    const why = lineWhy("R. Halvorsen", "black", LINE);
    expect(why).toContain("21 games");
    expect(why).toContain("38.0%");
    expect(why).toContain("Nc6");
    expect(why).toContain("book-exit");
    expect(lineName(LINE)).toBe("Sicilian, Alapin");
    expect(lineScore(LINE)).toBe("38% in 21");
  });

  it("stays honest for out-of-book spots", () => {
    expect(lineName({ ...LINE, eco: null, openingName: null })).toBe("Out of book");
  });
});

describe("fingerprint helpers", () => {
  const row: FingerprintRow = { eco: "B22", name: "Sicilian, Alapin", games: 21, sharePct: 14, scorePct: 38 };
  it("weak rule needs both a bad score and a real sample", () => {
    expect(fingerprintRowWeak(row)).toBe(true);
    expect(fingerprintRowWeak({ ...row, scorePct: 55 })).toBe(false);
    expect(fingerprintRowWeak({ ...row, games: 2 })).toBe(false);
  });
  it("matches book exits by ECO code", () => {
    const exits: BookExit[] = [
      { hash: "9", eco: "B22", openingName: null, san: "Nc6", ply: 5, count: 17, scorePct: 38 },
    ];
    expect(bookExitFor(row, exits)).toBe("leaves book at Nc6 (ply 5, 17×)");
    expect(bookExitFor({ ...row, eco: "C18" }, exits)).toBeNull();
  });
});

describe("prep state recording", () => {
  it("upserts one entry per opponent+color, newest first, backend timestamp format", () => {
    const now = new Date(Date.UTC(2026, 6, 26, 12, 0, 0));
    const first = recordPrep([], "R. Halvorsen", "black", now);
    expect(first).toEqual([{ opponent: "R. Halvorsen", color: "black", startedAt: "2026-07-26 12:00:00" }]);
    // Same opponent+color again: replaced, not duplicated.
    const again = recordPrep(first, "R. Halvorsen", "black", now);
    expect(again).toHaveLength(1);
    // A different color is a separate prep.
    expect(recordPrep(first, "R. Halvorsen", "white", now)).toHaveLength(2);
    expect(prepTimestamp(now)).toBe("2026-07-26 12:00:00");
  });
});

describe("the aside's profile finding", () => {
  it("uses the opponent's own profile when it exists", () => {
    const f = prepFinding("sounix", PROFILE_FIXTURE);
    expect(f).toContain("exposed kings");
    expect(f).toContain("11×");
  });
  it("is honestly absent otherwise — never someone else's findings", () => {
    expect(prepFinding("R. Halvorsen", PROFILE_FIXTURE)).toContain("No profile has been built");
    expect(prepFinding("R. Halvorsen", null)).toContain("No profile has been built");
    expect(prepFinding(null, null)).toContain("Pick an opponent");
  });
});
