import { describe, expect, it } from "vitest";
import { evidenceFromRecord, type FeatureRecordJson } from "./explainView";
import { evidenceView } from "./evidence";

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
        overlapping: ["c6"], // also an alert target: roles stack now
      },
      plans: [{ hint: "blockade", squares: ["d6", "x9"] }], // x9 invalid
    },
  ],
};

describe("evidenceFromRecord", () => {
  it("maps targets, attackers, defenders, imbalance evidence and plan squares", () => {
    const ev = evidenceFromRecord(record);
    expect(ev.alerts).toEqual(["c6"]);
    expect(ev.attackers.sort()).toEqual(["d4", "e5"]);
    expect(ev.defenders).toEqual(["b7"]);
    expect(ev.imbalance.sort()).toEqual(["c6", "d5"]);
    expect(ev.key).toEqual(["d6"]);
  });

  it("emits attacker→target arrows only (defenders get no arrow)", () => {
    const ev = evidenceFromRecord(record);
    expect(ev.arrows.map((a) => `${a.from}>${a.to}:${a.kind}`).sort()).toEqual([
      "d4>c6:attacker",
      "e5>c6:attacker",
    ]);
  });

  it("feeds the shared overlay module: roles stack on one square", () => {
    const view = evidenceView(evidenceFromRecord(record));
    const c6 = view.marks.filter((m) => m.square === "c6").map((m) => m.role);
    // c6 is both imbalance evidence and an alert target: wash under ring.
    expect(c6).toEqual(["imbalance", "alert-target"]);
    expect(view.shapes.map((s) => s.brush)).toEqual(["attacker", "attacker"]);
  });

  it("returns empty evidence for a quiet record", () => {
    const quiet: FeatureRecordJson = {
      ...record,
      wsui: { screen_fired: false, alerts: [] },
      imbalances: [],
    };
    expect(evidenceFromRecord(quiet)).toEqual({
      alerts: [],
      attackers: [],
      defenders: [],
      imbalance: [],
      key: [],
      arrows: [],
    });
  });
});
