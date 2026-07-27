import { describe, expect, it } from "vitest";
import type { PlayerProfile } from "./db";
import { PROFILE_FIXTURE as FIXTURE } from "./profileFixture";
import {
  claimId,
  claimTarget,
  parseClaim,
  profileLede,
  rankedMotifs,
  ratePct,
  trainableMotif,
} from "./profileView";

describe("parseClaim / claimId", () => {
  it("round-trips the navigation contract formats", () => {
    for (const id of [
      "motif:WeakKing:missed",
      "motif:Undefended:allowed",
      "structure:own-isolated-pawn",
      "phase:middlegame",
      "rate:conversion",
    ]) {
      const c = parseClaim(id);
      expect(c).not.toBeNull();
      expect(claimId(c!)).toBe(id);
    }
  });

  it("rejects garbage without throwing", () => {
    expect(parseClaim(null)).toBeNull();
    expect(parseClaim("")).toBeNull();
    expect(parseClaim("motif:WeakKing")).toBeNull();
    expect(parseClaim("motif:WeakKing:taken")).toBeNull();
    expect(parseClaim("bogus:thing")).toBeNull();
  });
});

describe("profileLede", () => {
  it("names the top finding (loudest motif) in plain language", () => {
    const lede = profileLede(FIXTURE);
    expect(lede).toContain("exposed kings"); // WeakKing humanized
    expect(lede).toContain("11×"); // its allowed count
    expect(lede).toContain("38%"); // the weak structure, second finding
    expect(lede).toContain("42 games");
  });

  it("stays honest when nothing stands out", () => {
    const empty: PlayerProfile = {
      ...FIXTURE,
      motifs: [],
      structures: [],
      conversion: { winning_reached: 0, converted_wins: 0, losing_reached: 0, held: 0 },
    };
    expect(profileLede(empty)).toContain("No dominant weakness");
  });
});

describe("claimTarget", () => {
  it("motif missed cell → its examples and real counts", () => {
    const t = claimTarget(FIXTURE, { kind: "motif", motif: "WeakKing", cell: "missed" });
    expect(t.countLabel).toBe("6 MISSED");
    expect(t.examples).toEqual([{ game: 7, ply: 43 }]);
    expect(t.intro).toContain("6 of 9");
  });

  it("structure claim → games count and score vs the 50% baseline", () => {
    const t = claimTarget(FIXTURE, { kind: "structure", flag: "own-isolated-pawn" });
    expect(t.countLabel).toBe("22 GAMES");
    expect(t.examples).toEqual([{ game: 10, ply: 30 }]);
    expect(t.intro).toContain("38.0%");
  });

  it("phase claims are honest about having no example list", () => {
    const t = claimTarget(FIXTURE, { kind: "phase", phase: "middlegame" });
    expect(t.countLabel).toBe("200 MOVES");
    expect(t.examples).toEqual([]);
    expect(t.emptyNote).toMatch(/no per-game example list/i);
  });
});

describe("small helpers", () => {
  it("rankedMotifs orders by missed+allowed", () => {
    expect(rankedMotifs(FIXTURE).map((m) => m.kind)).toEqual(["WeakKing", "Undefended"]);
  });
  it("ratePct never fakes a number on a zero denominator", () => {
    expect(ratePct(5, 0)).toBe("—");
    expect(ratePct(26, 41)).toBe("63%");
  });
  it("only motif claims are trainable", () => {
    expect(trainableMotif({ kind: "motif", motif: "WeakKing", cell: "missed" })).toBe("WeakKing");
    expect(trainableMotif({ kind: "structure", flag: "x" })).toBeNull();
    expect(trainableMotif(null)).toBeNull();
  });
});
