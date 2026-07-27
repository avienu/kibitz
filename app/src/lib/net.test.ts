import { describe, expect, it } from "vitest";
import {
  formatReport,
  missingIssues,
  netStripProgress,
  type NetProgress,
  type TwicCatalogRow,
} from "./net";

const row = (issue: number, imported: boolean): TwicCatalogRow => ({
  issue,
  imported,
  games: imported ? 10 : null,
  approxDate: "2025-01-06",
});

describe("missingIssues", () => {
  it("keeps only not-yet-imported issues, in row order", () => {
    const rows = [row(1652, false), row(1651, true), row(1650, false)];
    expect(missingIssues(rows)).toEqual([1652, 1650]);
  });

  it("is empty when everything is imported", () => {
    expect(missingIssues([row(1650, true)])).toEqual([]);
  });
});

describe("formatReport", () => {
  it("returns null without a report — no fake history", () => {
    expect(formatReport(null)).toBeNull();
  });

  it("renders a success report with counts", () => {
    expect(
      formatReport({
        at: "2026-07-27 12:00:00",
        gamesImported: 128,
        duplicatesSkipped: 40,
        gamesFailed: 1,
      }),
    ).toBe("Last sync 2026-07-27 12:00:00 UTC: 128 imported · 40 duplicates · 1 failed");
  });

  it("appends chess.com months and FICS year/month when present", () => {
    expect(
      formatReport({ at: "t", gamesImported: 2, duplicatesSkipped: 0, gamesFailed: 0, monthsFetched: 4 }),
    ).toContain("4 month(s)");
    expect(
      formatReport({ at: "t", gamesImported: 2, duplicatesSkipped: 0, gamesFailed: 0, year: 2025, month: 6 }),
    ).toContain("2025-06");
    expect(
      formatReport({ at: "t", gamesImported: 2, duplicatesSkipped: 0, gamesFailed: 0, year: 2025, month: null }),
    ).toContain("2025");
  });

  it("surfaces a stored error honestly", () => {
    expect(formatReport({ at: "t", error: "HTTP 500 for …" })).toBe("Failed (t UTC): HTTP 500 for …");
  });
});

describe("netStripProgress (status-strip cell)", () => {
  const base: NetProgress = {
    kind: "twic",
    label: "TWIC download",
    done: 3,
    total: 10,
    detail: "",
    active: true,
    error: null,
  };

  it("shows a fraction only for an active TWIC job", () => {
    expect(netStripProgress(base)).toEqual({ label: "TWIC DOWNLOAD", fraction: 0.3 });
    expect(netStripProgress({ ...base, kind: "twic-auto" })).toEqual({
      label: "TWIC AUTO-SYNC",
      fraction: 0.3,
    });
  });

  it("never fakes a fraction for indeterminate account syncs", () => {
    expect(netStripProgress({ ...base, kind: "lichess", total: 0 })).toBeNull();
    // Even a (buggy) nonzero total on an account sync gets no cell.
    expect(netStripProgress({ ...base, kind: "chesscom" })).toBeNull();
  });

  it("is absent when idle or without progress data", () => {
    expect(netStripProgress(null)).toBeNull();
    expect(netStripProgress({ ...base, active: false })).toBeNull();
    expect(netStripProgress({ ...base, total: 0 })).toBeNull();
  });
});
