import { describe, expect, it } from "vitest";
import type { PlayerProfile } from "./db";
import { formatCount, profileFindings, railBadge, RAIL_FOOTER, RAIL_GROUPS } from "./shell";

const noData = {
  dbGames: null,
  explainOn: false,
  profile: null,
  train: null,
  tactics: null,
  jobs: null,
  twicLatestImported: null,
  syncAccounts: null,
};

describe("rail structure", () => {
  it("carries every capability a home (the discoverability fix)", () => {
    const ids = RAIL_GROUPS.flatMap((g) => g.items.map((i) => i.id));
    expect(RAIL_GROUPS.map((g) => g.heading)).toEqual([
      "STUDY",
      "COACH",
      "TRAIN",
      "DATA IN / OUT",
    ]);
    expect(ids).toEqual([
      "database",
      "game",
      "tree",
      "search",
      "home",
      "explain",
      "profile",
      "prep",
      "train",
      "triage",
      "tactics",
      "endgames",
      "play",
      "import",
      "twic",
      "syncs",
      "jobs",
    ]);
    expect(RAIL_FOOTER.map((i) => i.id)).toEqual(["settings", "help"]);
  });
});

describe("badges (real data or nothing)", () => {
  it("shows nothing without data — never fake numbers", () => {
    for (const id of ["database", "profile", "train", "tactics", "jobs", "twic", "syncs"] as const) {
      expect(railBadge(id, noData)).toBeNull();
    }
  });

  it("formats the db game count compactly", () => {
    expect(formatCount(121_438)).toBe("121k");
    expect(formatCount(10_000)).toBe("10k");
    expect(formatCount(9_999)).toBe("9999");
    expect(railBadge("database", { ...noData, dbGames: 121_438 })).toBe("121k");
  });

  it("explain badge tracks the toggle", () => {
    expect(railBadge("explain", noData)).toBe("off");
    expect(railBadge("explain", { ...noData, explainOn: true })).toBe("on");
  });

  it("SRS due sums both colors", () => {
    const train = { white: { due: 14, total: 40 }, black: { due: 10, total: 22 } };
    expect(railBadge("train", { ...noData, train })).toBe("24 due");
  });

  it("jobs badge prefers running > pending > done", () => {
    const jobs = { pending: 0, running: 0, done: 264, failed: 0, workerActive: false, engine: null };
    expect(railBadge("jobs", { ...noData, jobs })).toBe("264");
    expect(railBadge("jobs", { ...noData, jobs: { ...jobs, pending: 3 } })).toBe("3 pending");
    expect(railBadge("jobs", { ...noData, jobs: { ...jobs, running: 2, pending: 3 } })).toBe(
      "2 running",
    );
    expect(
      railBadge("jobs", { ...noData, jobs: { ...jobs, done: 0 } }),
    ).toBeNull();
  });

  it("twic badge shows the newest IMPORTED week (rail_net_badges)", () => {
    expect(railBadge("twic", { ...noData, twicLatestImported: 1650 })).toBe("wk 1650");
    expect(railBadge("twic", noData)).toBeNull();
  });

  it("syncs badge counts configured accounts, hidden at zero", () => {
    expect(railBadge("syncs", { ...noData, syncAccounts: 2 })).toBe("2 linked");
    expect(railBadge("syncs", { ...noData, syncAccounts: 0 })).toBeNull();
    expect(railBadge("syncs", noData)).toBeNull();
  });

  it("profile findings use the prep weakness rules", () => {
    const profile = {
      player: "X",
      games: 20,
      score_pct: 50,
      eval_coverage_pct: 80,
      acpl_opening: { moves: 1, acpl: 20, blunders: 0, mistakes: 0, inaccuracies: 0 },
      acpl_middlegame: { moves: 1, acpl: 20, blunders: 0, mistakes: 0, inaccuracies: 0 },
      acpl_endgame: { moves: 1, acpl: 20, blunders: 0, mistakes: 0, inaccuracies: 0 },
      motifs: [
        { kind: "fork", opportunities: 4, taken: 1, missed: 2, allowed: 1, example_missed: [], example_allowed: [] },
        { kind: "pin", opportunities: 2, taken: 2, missed: 0, allowed: 0, example_missed: [], example_allowed: [] },
      ],
      structures: [
        { flag: "iqp", games: 5, score_pct: 30, examples: [] },
        { flag: "open", games: 1, score_pct: 10, examples: [] },
      ],
      eco: [],
      conversion: { winning_reached: 0, converted_wins: 0, losing_reached: 0, held: 0 },
    } satisfies PlayerProfile;
    expect(profileFindings(profile)).toBe(2); // 1 motif with misses + 1 structure ≥2 games under 50%
    expect(railBadge("profile", { ...noData, profile })).toBe("2 findings");
    expect(profileFindings(null)).toBeNull();
  });
});
