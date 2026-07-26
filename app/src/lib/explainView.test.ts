import { describe, expect, it } from "vitest";
import { shapesFromRecord, type FeatureRecordJson } from "./explainView";

const record: FeatureRecordJson = {
  schema_version: 1,
  fen: "rnbqkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 2",
  side_to_move: "white",
  phase: "middlegame",
  wsui: {
    screen_fired: true,
    alerts: [
      {
        kind: "InadequatelyDefended",
        side: "black",
        target: "c6",
        attackers: ["e5", "d4"],
        defenders: ["b7"],
        severity: "high",
      },
      { kind: "WeakKing", side: "white", severity: "low" }, // no target: diffuse
    ],
  },
  imbalances: [
    {
      kind: "PawnStructure",
      favors: "white",
      magnitude: "clear",
      evidence: {
        isolated: ["d5"],
        half_open_files: ["d"], // not squares: must be ignored
        count: 2, // not an array: ignored
        overlapping: ["c6"], // already an alert target: red wins
      },
    },
  ],
};

describe("shapesFromRecord", () => {
  it("colors targets red, attackers orange, evidence green", () => {
    const shapes = shapesFromRecord(record);
    const byBrush = (brush: string) =>
      shapes
        .filter((s) => s.brush === brush)
        .map((s) => s.orig)
        .sort();
    expect(byBrush("red")).toEqual(["c6"]);
    expect(byBrush("orange")).toEqual(["d4", "e5"]);
    expect(byBrush("green")).toEqual(["d5"]);
    // No duplicate squares: red > orange > green precedence.
    expect(new Set(shapes.map((s) => s.orig)).size).toBe(shapes.length);
  });

  it("returns nothing for a quiet record", () => {
    const quiet: FeatureRecordJson = {
      ...record,
      wsui: { screen_fired: false, alerts: [] },
      imbalances: [],
    };
    expect(shapesFromRecord(quiet)).toEqual([]);
  });
});
