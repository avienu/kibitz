import { describe, expect, it } from "vitest";
import {
  evalsByPly,
  formatWhiteCp,
  legacyEvalTitle,
  whitePovCp,
  type AnalysisRow,
} from "./analyses";

function row(ply: number, kind: string, evalCp: number, engine = "Stockfish 17"): AnalysisRow {
  return { ply, kind, engine, depth: null, nodes: 200000, evalCp, createdAt: "2026-07-26" };
}

describe("whitePovCp", () => {
  it("negates fresh evals at odd plies (Black to move)", () => {
    // After 1 ply Black is to move: +80 for Black = -80 for White.
    expect(whitePovCp("fresh", 1, 80)).toBe(-80);
    expect(whitePovCp("fresh", 3, -250)).toBe(250);
  });

  it("keeps fresh evals at even plies (White to move)", () => {
    expect(whitePovCp("fresh", 2, 80)).toBe(80);
    expect(whitePovCp("fresh", 0, -30)).toBe(-30);
  });

  it("never converts legacy imports (already White-POV)", () => {
    expect(whitePovCp("legacy-import", 1, 80)).toBe(80);
    expect(whitePovCp("legacy-import", 2, -120)).toBe(-120);
  });
});

describe("evalsByPly", () => {
  it("prefers fresh over legacy at the same ply, regardless of order", () => {
    const m = evalsByPly([
      row(3, "legacy-import", 50, "Rybka 4"),
      row(3, "fresh", -40),
      row(4, "fresh", 25),
    ]);
    expect(m.get(3)).toEqual({ whiteCp: 40, kind: "fresh", engine: "Stockfish 17" });
    expect(m.get(4)).toEqual({ whiteCp: 25, kind: "fresh", engine: "Stockfish 17" });
  });

  it("keeps the first row within a kind (rows arrive newest first)", () => {
    const m = evalsByPly([row(2, "fresh", 10, "SF new"), row(2, "fresh", 99, "SF old")]);
    expect(m.get(2)).toEqual({ whiteCp: 10, kind: "fresh", engine: "SF new" });
  });

  it("falls back to legacy when no fresh row exists", () => {
    const m = evalsByPly([row(5, "legacy-import", -70, "Rybka 4")]);
    expect(m.get(5)).toEqual({ whiteCp: -70, kind: "legacy", engine: "Rybka 4" });
    expect(m.get(6)).toBeUndefined();
  });
});

describe("formatWhiteCp", () => {
  it("renders pawn units with an explicit sign", () => {
    expect(formatWhiteCp(40)).toBe("+0.4");
    expect(formatWhiteCp(-125)).toBe("-1.3");
    expect(formatWhiteCp(0)).toBe("+0.0");
    expect(formatWhiteCp(10000)).toBe("+100.0");
  });
});

describe("legacyEvalTitle", () => {
  it("names the vintage engine", () => {
    expect(legacyEvalTitle("Rybka 4 x64")).toBe(
      "Engine Vintage: Rybka 4 x64, imported analysis",
    );
  });
});
