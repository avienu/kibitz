import { describe, expect, it } from "vitest";
import type { Commitment, HomeFinding, HomeNewGame, PrepEntry } from "./db";
import {
  commitmentClause,
  findingDotTone,
  findingsProse,
  fmtDurationMs,
  greetingDate,
  isDegraded,
  newSinceLabel,
  rangeReadout,
  sourceTagTone,
} from "./home";

const prep = (opponent: string): PrepEntry => ({
  opponent,
  color: "black",
  startedAt: "2026-07-20 19:00:00",
});

describe("greetingDate", () => {
  it("renders the serif date", () => {
    expect(greetingDate(new Date(2026, 6, 26))).toBe("Sunday, 26 July");
  });
});

describe("commitmentClause (maintainer honesty rules)", () => {
  it("is ABSENT when no commitment label is set", () => {
    expect(commitmentClause(null, [])).toBeNull();
    expect(commitmentClause({ label: null, opponent: null }, [])).toBeNull();
    // Opponent without a label still renders nothing — the clause hangs
    // off the commitment, not the opponent.
    expect(commitmentClause({ label: null, opponent: "R. Halvorsen" }, [])).toBeNull();
  });

  it("says 'no prep started' only when an opponent is set AND unprepped", () => {
    const c: Commitment = { label: "Club night Thursday", opponent: "R. Halvorsen" };
    expect(commitmentClause(c, [])).toBe(
      "Club night Thursday — no prep started for R. Halvorsen yet.",
    );
    expect(commitmentClause(c, [prep("R. Halvorsen")])).toBe("Club night Thursday.");
    // Prep for someone else doesn't count.
    expect(commitmentClause(c, [prep("M. Sæther")])).toBe(
      "Club night Thursday — no prep started for R. Halvorsen yet.",
    );
  });

  it("renders the bare label when no opponent is named", () => {
    expect(commitmentClause({ label: "Club night Thursday", opponent: null }, [])).toBe(
      "Club night Thursday.",
    );
  });
});

describe("sourceTagTone", () => {
  it("maps kinds to the design's tag colours", () => {
    expect(sourceTagTone("personal", "my games")).toBe("accent");
    expect(sourceTagTone("twic", "TWIC 1601")).toBe("info");
    expect(sourceTagTone("online", "lichess sync")).toBe("violet");
    expect(sourceTagTone("online", "chess.com rapid")).toBe("good");
    expect(sourceTagTone("online", "FICS")).toBe("dim");
    expect(sourceTagTone("other", "misc")).toBe("dim");
  });
});

describe("newSinceLabel", () => {
  const g = (id: number, importedAt: string): HomeNewGame => ({
    id,
    white: "a",
    black: "b",
    result: "1-0",
    source: "s",
    sourceKind: "personal",
    importedAt,
  });

  it("names the weekday of the oldest import", () => {
    // 2026-07-24 is a Friday.
    const label = newSinceLabel(
      { newGames: [g(2, "2026-07-26 06:00:00"), g(1, "2026-07-24 06:00:00")] },
      "UTC",
    );
    expect(label).toBe("NEW SINCE FRIDAY");
  });

  it("labels the USER's weekday, not UTC's (audit #10)", () => {
    // 01:30 UTC Saturday is still Friday evening in Los Angeles.
    const games = { newGames: [g(1, "2026-07-25 01:30:00")] };
    expect(newSinceLabel(games, "UTC")).toBe("NEW SINCE SATURDAY");
    expect(newSinceLabel(games, "America/Los_Angeles")).toBe("NEW SINCE FRIDAY");
  });

  it("is null with no new games (panel absent, not faked)", () => {
    expect(newSinceLabel({ newGames: [] })).toBeNull();
  });
});

describe("findingsProse", () => {
  const f = (label: string, value: string): HomeFinding => ({
    label,
    value,
    evidenceCount: 3,
    claimId: "motif:Fork:allowed",
  });

  it("names the top two findings", () => {
    expect(
      findingsProse([f("Fork — allowed against you", "31"), f("IQP games", "38%")]),
    ).toBe("Your biggest leaks are fork — allowed against you (31) and iqp games (38%).");
  });

  it("handles a single finding and none", () => {
    expect(findingsProse([f("Fork — missed opportunities", "5")])).toBe(
      "Your biggest leak is fork — missed opportunities (5).",
    );
    expect(findingsProse([])).toBeNull();
  });
});

describe("findingDotTone", () => {
  it("motif claims are weaknesses, structure claims score readings", () => {
    expect(findingDotTone("motif:Fork:allowed")).toBe("bad");
    expect(findingDotTone("structure:IQP")).toBe("good");
  });
});

describe("isDegraded", () => {
  const base = { dueSrs: 0, newGames: [], findingsAvailable: false };
  it("requires nothing due, no new games, no findings, no commitment", () => {
    expect(isDegraded(base, null)).toBe(true);
    expect(isDegraded(base, { label: null, opponent: null })).toBe(true);
    expect(isDegraded({ ...base, dueSrs: 3 }, null)).toBe(false);
    expect(isDegraded({ ...base, findingsAvailable: true }, null)).toBe(false);
    expect(isDegraded(base, { label: "Club night", opponent: null })).toBe(false);
  });
});

describe("fmtDurationMs / rangeReadout", () => {
  it("formats durations honestly at every scale", () => {
    expect(fmtDurationMs(4_000)).toBe("~4 s");
    expect(fmtDurationMs(150_000)).toBe("~3 m");
    expect(fmtDurationMs(7_800_000)).toBe("~2 h 10 m");
    expect(fmtDurationMs(7_200_000)).toBe("~2 h");
  });

  it("formats the range readout", () => {
    expect(rangeReadout(0, 50, 121_438)).toBe("1–50 of 121,438");
    expect(rangeReadout(50, 50, 121_438)).toBe("51–100 of 121,438");
    expect(rangeReadout(0, 0, 0)).toBe("0 games");
  });
});
