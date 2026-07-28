import { describe, expect, it } from "vitest";
import {
  buildCrosstable,
  crosstableEligible,
  formatPoints,
  parseRound,
  type CrosstableGameRow,
} from "./crosstable";

function game(partial: Partial<CrosstableGameRow> & { id: number }): CrosstableGameRow {
  return {
    white: "W",
    black: "B",
    whiteElo: null,
    blackElo: null,
    round: null,
    result: "*",
    date: null,
    ...partial,
  };
}

/** The kibitz-db fixture's Mini RR (crosstable.rs) mirrored here. */
const MINI_RR: CrosstableGameRow[] = [
  game({
    id: 1,
    white: "Alpha",
    black: "Bravo",
    whiteElo: 2400,
    blackElo: 2300,
    round: "1",
    result: "1-0",
  }),
  game({
    id: 2,
    white: "Bravo",
    black: "Charlie",
    whiteElo: 2300,
    blackElo: 2200,
    round: "2",
    result: "1/2-1/2",
  }),
  game({
    id: 3,
    white: "Charlie",
    black: "Alpha",
    whiteElo: 2200,
    blackElo: 2400,
    round: "3",
    result: "0-1",
  }),
];

describe("parseRound", () => {
  it("buckets '1', '1.2' and zero-padded tags into major rounds", () => {
    expect(parseRound("1")).toBe(1);
    expect(parseRound("1.2")).toBe(1);
    expect(parseRound("03")).toBe(3);
    expect(parseRound(" 7 ")).toBe(7);
  });

  it("sends '?', blanks, dashes and junk to the unrounded bucket — never throws", () => {
    for (const junk of ["?", "", "-", "A", "1a", "?.?", null, undefined]) {
      expect(parseRound(junk), String(junk)).toBeNull();
    }
  });
});

describe("buildCrosstable — round robin", () => {
  it("lays out players × rounds with per-perspective scores", () => {
    const t = buildCrosstable(MINI_RR);
    expect(t.mode).toBe("grid");
    expect(t.rounds).toEqual([1, 2, 3]);
    expect(t.hasUnrounded).toBe(false);
    expect(t.games).toBe(3);

    // Standings: Alpha 2; Bravo and Charlie tie at ½ — the perf
    // tiebreak puts Charlie (2150: ¼ vs avg 2350) above Bravo (2100).
    expect(t.players.map((p) => p.name)).toEqual(["Alpha", "Charlie", "Bravo"]);
    const alpha = t.players[0];
    expect(alpha.points).toBe(2);
    expect(alpha.counted).toBe(2);
    expect(alpha.elo).toBe(2400);
    // Round 1: won as White vs Bravo; round 2 empty (bye); round 3 won as Black.
    expect(alpha.cells[0]).toEqual([{ gameId: 1, opponent: "Bravo", score: "1", color: "w" }]);
    expect(alpha.cells[1]).toEqual([]);
    expect(alpha.cells[2]).toEqual([{ gameId: 3, opponent: "Charlie", score: "1", color: "b" }]);

    // Perf: Alpha scored 2/2 vs avg 2250 → 2250 + 800·1 − 400 = 2650.
    expect(alpha.perf).toBe(2650);
    // Charlie: ½ from (0 vs 2400, ½ vs 2300) → avg 2350 + 800·0.25 − 400 = 2150.
    expect(t.players[1].perf).toBe(2150);
    // Bravo: ½ from (0 vs 2400, ½ vs 2200) → avg 2300 + 800·0.25 − 400 = 2100.
    expect(t.players[2].perf).toBe(2100);
  });
});

describe("buildCrosstable — ragged rounds", () => {
  it("buckets unparseable rounds into a trailing column and keeps '*' scoreless", () => {
    const games = [
      game({ id: 1, white: "Delta", black: "Echo", round: "1.2", result: "1-0" }),
      game({ id: 2, white: "Echo", black: "Foxtrot", round: "?", result: "0-1" }),
      game({ id: 3, white: "Foxtrot", black: "Delta", round: null, result: "*" }),
    ];
    const t = buildCrosstable(games);
    // 1 of 3 parseable < half → the Swiss degrade to the scored list.
    expect(t.mode).toBe("list");
    expect(t.hasUnrounded).toBe(true);
    const foxtrot = t.players.find((p) => p.name === "Foxtrot")!;
    expect(foxtrot.points).toBe(1);
    expect(foxtrot.counted).toBe(1);
    expect(foxtrot.games).toBe(2); // the '*' game still counts as played
    expect(foxtrot.perf).toBeNull(); // no rated opponents — no fake number
    // The unfinished game sits in the trailing bucket with score "*".
    const bucket = foxtrot.cells[foxtrot.cells.length - 1];
    expect(bucket.some((c) => c.score === "*" && c.gameId === 3)).toBe(true);
  });

  it("stays in grid mode when at least half the games have rounds", () => {
    const games = [
      game({ id: 1, white: "A", black: "B", round: "1", result: "1-0" }),
      game({ id: 2, white: "B", black: "A", round: "2", result: "1-0" }),
      game({ id: 3, white: "A", black: "B", round: "?", result: "1/2-1/2" }),
    ];
    const t = buildCrosstable(games);
    expect(t.mode).toBe("grid");
    expect(t.rounds).toEqual([1, 2]);
    expect(t.hasUnrounded).toBe(true);
    // Every row carries rounds.length + 1 cell columns.
    for (const p of t.players) expect(p.cells).toHaveLength(3);
  });

  it("never crashes on an empty event", () => {
    const t = buildCrosstable([]);
    expect(t.mode).toBe("list");
    expect(t.players).toEqual([]);
    expect(t.games).toBe(0);
  });
});

describe("helpers", () => {
  it("formats points the chess way", () => {
    expect(formatPoints(0)).toBe("0");
    expect(formatPoints(0.5)).toBe("½");
    expect(formatPoints(2)).toBe("2");
    expect(formatPoints(2.5)).toBe("2½");
  });

  it("refuses crosstables for anonymous events", () => {
    expect(crosstableEligible("Tata Steel")).toBe(true);
    expect(crosstableEligible("?")).toBe(false);
    expect(crosstableEligible("")).toBe(false);
    expect(crosstableEligible(null)).toBe(false);
  });
});
