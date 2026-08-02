import { describe, expect, it } from "vitest";
import {
  formatReport,
  idleLine,
  missingIssues,
  netStripProgress,
  type NetProgress,
  type ServiceAccount,
  type TwicCatalogRow,
  syncRunLine,
  type SyncRun,
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

  it("renders the per-run counts (the timestamp lives on the idle line)", () => {
    const report = {
      at: "2026-07-27 12:00:00",
      gamesImported: 128,
      duplicatesSkipped: 40,
      gamesFailed: 1,
    };
    expect(formatReport(report)).toBe(
      "Last run: 128 imported · 40 duplicates · 1 failed",
    );
  });

  it("failure lines carry a LOCAL timestamp (audit #10)", () => {
    const report = { at: "2026-07-27 12:00:00", error: "HTTP 500" };
    expect(formatReport(report, "UTC")).toBe(
      "Failed (2026-07-27 12:00): HTTP 500",
    );
    expect(formatReport(report, "America/Los_Angeles")).toBe(
      "Failed (2026-07-27 05:00): HTTP 500",
    );
  });

  it("appends chess.com months and FICS year/month when present", () => {
    expect(
      formatReport({
        at: "t",
        gamesImported: 2,
        duplicatesSkipped: 0,
        gamesFailed: 0,
        monthsFetched: 4,
      }),
    ).toContain("4 month(s)");
    expect(
      formatReport({
        at: "t",
        gamesImported: 2,
        duplicatesSkipped: 0,
        gamesFailed: 0,
        year: 2025,
        month: 6,
      }),
    ).toContain("2025-06");
    expect(
      formatReport({
        at: "t",
        gamesImported: 2,
        duplicatesSkipped: 0,
        gamesFailed: 0,
        year: 2025,
        month: null,
      }),
    ).toContain("2025");
  });

  it("surfaces a stored error honestly (malformed timestamps unmangled)", () => {
    expect(formatReport({ at: "t", error: "HTTP 500 for …" })).toBe(
      "Failed (t): HTTP 500 for …",
    );
  });
});

describe("idleLine (audit #16/#21: the card states what it has done)", () => {
  const account = (over: Partial<ServiceAccount>): ServiceAccount => ({
    username: "SomeUser",
    lastReport: null,
    gamesTotal: 0,
    ...over,
  });

  it("shows last-synced LOCAL time and the live total", () => {
    const a = account({
      lastReport: { at: "2026-07-26 21:00:00", gamesImported: 5 },
      gamesTotal: 12345,
    });
    expect(idleLine(a, "UTC")).toBe(
      "Last synced 2026-07-26 21:00 · 12,345 games imported total",
    );
    expect(idleLine(a, "America/Los_Angeles")).toBe(
      "Last synced 2026-07-26 14:00 · 12,345 games imported total",
    );
  });

  it("still reports totals when no sync report survives", () => {
    expect(idleLine(account({ gamesTotal: 1 }))).toBe(
      "No sync recorded · 1 game imported total",
    );
  });

  it("is null only when there is truly nothing to say", () => {
    expect(idleLine(null)).toBeNull();
    expect(idleLine(account({}))).toBeNull();
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
    queued: [],
  };

  it("shows a fraction only for an active TWIC job", () => {
    expect(netStripProgress(base)).toEqual({
      label: "TWIC DOWNLOAD",
      fraction: 0.3,
    });
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

describe("syncRunLine — a run that found nothing is still an event", () => {
  const run = (over: Partial<SyncRun> = {}): SyncRun => ({
    id: 1,
    service: "twic",
    trigger: "auto",
    startedAt: "2026-08-01 02:25:20",
    finishedAt: "2026-08-01 02:25:29",
    gamesImported: 0,
    duplicatesSkipped: 0,
    gamesFailed: 0,
    detail: null,
    error: null,
    ...over,
  });

  it("says 'nothing new' rather than going silent", () => {
    expect(syncRunLine(run(), "UTC")).toContain("automatic · nothing new");
  });

  it("prefers the run's own summary when it has one", () => {
    const line = syncRunLine(
      run({ detail: "up to date — TWIC 1655 is newest" }),
      "UTC",
    );
    expect(line).toContain("up to date — TWIC 1655 is newest");
  });

  it("reports counts, and only the ones that happened", () => {
    expect(
      syncRunLine(run({ trigger: "manual", gamesImported: 12 }), "UTC"),
    ).toContain("manual · 12 imported");
    const full = syncRunLine(
      run({ gamesImported: 12, duplicatesSkipped: 3, gamesFailed: 1 }),
      "UTC",
    );
    expect(full).toContain("12 imported · 3 already had · 1 failed");
  });

  it("distinguishes failed from interrupted from done", () => {
    expect(syncRunLine(run({ error: "429" }), "UTC")).toContain("failed: 429");
    expect(syncRunLine(run({ finishedAt: null }), "UTC")).toContain(
      "interrupted",
    );
  });
});
