import { describe, expect, it } from "vitest";
import { reachingEmptyCopy, treeEmptyCopy, treePhase } from "./treeView";

describe("treePhase", () => {
  it("is closed while no database is open, whatever else is happening", () => {
    expect(treePhase(false, false, null)).toBe("closed");
    expect(treePhase(false, true, "boom")).toBe("closed");
  });

  it("reports an error even when a newer fetch is already in flight", () => {
    expect(treePhase(true, true, "database is locked")).toBe("error");
  });

  it("is loading while the query is in flight and settled after", () => {
    expect(treePhase(true, true, null)).toBe("loading");
    expect(treePhase(true, false, null)).toBe("settled");
  });
});

describe("empty-slot copy (audit #2)", () => {
  it("reserves the true-empty claim for a settled, successful query", () => {
    // The audit's exact failure: a pending query during TWIC sync rendered
    // "No database moves from this position" — a claim about the database
    // that the query had not yet earned.
    expect(treeEmptyCopy("loading")).not.toContain("No database moves");
    expect(treeEmptyCopy("error")).not.toContain("No database moves");
    expect(treeEmptyCopy("closed")).not.toContain("No database moves");
    expect(treeEmptyCopy("settled")).toBe("No database moves from this position.");
  });

  it("names the error state explicitly", () => {
    expect(treeEmptyCopy("error")).toContain("see the error above");
  });

  it("applies the same discipline to the games-reaching aside", () => {
    expect(reachingEmptyCopy("loading")).toBe("Loading games…");
    expect(reachingEmptyCopy("error")).not.toContain("No games reach");
    expect(reachingEmptyCopy("settled")).toBe("No games reach this exact position.");
  });
});
