import { describe, expect, it } from "vitest";
import {
  DEFAULT_INTENSITY,
  EVIDENCE_COLORS,
  ROLE_PAINT_ORDER,
  arrowPolygonPoints,
  arrowStrokeWidth,
  boardGeometry,
  evidenceView,
  snapBoardSize,
  squareCenter,
  type Evidence,
} from "./evidence";

/** Sample evidence exercising every role (loosely the Morphy d7 mock). */
const ev: Evidence = {
  alerts: ["d7"],
  attackers: ["b5", "d1"],
  defenders: ["e8", "e7"],
  imbalance: ["c6", "d5"],
  key: ["d5"],
  arrows: [
    { from: "d1", to: "d7", kind: "key" },
    { from: "b5", to: "d7", kind: "attacker" },
    { from: "d1", to: "d7", kind: "attacker" }, // same from|to as the key arrow
    { from: "e8", to: "d7", kind: "defender" },
    { from: "b5", to: "d7", kind: "alert" }, // same from|to as an attacker arrow
  ],
};

describe("evidenceView arrows", () => {
  it("dedupes by from|to with first role winning (alert/attacker before key)", () => {
    const { shapes } = evidenceView(ev);
    const byPair = new Map(shapes.map((s) => [`${s.orig}|${s.dest}`, s.brush]));
    expect(byPair.get("b5|d7")).toBe("alert"); // alert beats attacker
    expect(byPair.get("d1|d7")).toBe("attacker"); // attacker beats key
    expect(byPair.get("e8|d7")).toBe("defender");
    expect(shapes).toHaveLength(3); // one arrow per from|to pair
  });

  it("names each arrow's role in `brush`, with an exact hue defined for it", () => {
    const { shapes } = evidenceView(ev);
    for (const s of shapes) {
      expect(["alert", "attacker", "defender", "imbalance", "key"]).toContain(s.brush);
      expect(EVIDENCE_COLORS[s.brush as keyof typeof EVIDENCE_COLORS].line).toMatch(/^#/);
    }
  });

  it("drops self-arrows and invalid squares", () => {
    const bad: Evidence = {
      ...ev,
      arrows: [
        { from: "d7", to: "d7", kind: "attacker" },
        { from: "z9", to: "d7", kind: "attacker" },
      ],
    };
    expect(evidenceView(bad).shapes).toEqual([]);
  });
});

describe("evidenceView intensity", () => {
  it("derives ring/wash/wedge/arrow opacities at the 0.44 baseline", () => {
    const v = evidenceView(ev); // default intensity
    expect(v.intensity).toBe(DEFAULT_INTENSITY);
    expect(v.ringOpacity).toBeCloseTo(0.42 + 0.5 * 0.44, 10); // 0.64
    expect(v.washOpacity).toBeCloseTo(0.5 + 0.5 * 0.44, 10); // 0.72
    expect(v.wedgeOpacity).toBeCloseTo(0.55 + 0.45 * 0.44, 10); // 0.748
    expect(v.arrowOpacity).toBeCloseTo(0.42 + 0.44 * 0.44, 10); // 0.6136
    const ring = v.marks.find((m) => m.role === "alert-target")!;
    const wash = v.marks.find((m) => m.role === "imbalance")!;
    const wedge = v.marks.find((m) => m.role === "attacker")!;
    expect(ring.opacity).toBeCloseTo(0.64, 10);
    expect(wash.opacity).toBeCloseTo(0.72, 10);
    expect(wedge.opacity).toBeCloseTo(0.748, 10);
  });

  it("derives opacities at 1.0 (hovered sentence)", () => {
    const v = evidenceView(ev, { intensity: 1 });
    expect(v.ringOpacity).toBeCloseTo(0.92, 10);
    expect(v.washOpacity).toBeCloseTo(1, 10);
    expect(v.wedgeOpacity).toBeCloseTo(1, 10);
    expect(v.arrowOpacity).toBeCloseTo(0.86, 10);
  });

  it("keeps last-move and selected marks at full element opacity", () => {
    const v = evidenceView(null, { lastMove: ["e2", "e4"], selected: "d7" });
    expect(v.marks.map((m) => [m.square, m.role, m.opacity])).toEqual([
      ["e2", "last-move", 1],
      ["e4", "last-move", 1],
      ["d7", "selected", 1],
    ]);
  });
});

describe("evidenceView paint order", () => {
  it("sorts marks bottom → top: last-move, imbalance, key, defender, attacker, alert, selected", () => {
    const stacked: Evidence = {
      alerts: ["d5"],
      attackers: ["d5"],
      defenders: ["d5"],
      imbalance: ["d5"],
      key: ["d5"],
      arrows: [],
    };
    const v = evidenceView(stacked, { lastMove: ["d5", "d5"], selected: "d5" });
    expect(v.marks.map((m) => m.role)).toEqual([
      "last-move",
      "imbalance",
      "key",
      "defender",
      "attacker",
      "alert-target",
      "selected",
    ]);
    const orders = v.marks.map((m) => ROLE_PAINT_ORDER[m.role]);
    expect(orders).toEqual([...orders].sort((a, b) => a - b));
  });
});

describe("evidenceView isolation", () => {
  it("keeps only evidence touching the isolation set (position state always renders)", () => {
    const v = evidenceView(ev, { isolate: new Set(["d7", "b5"]), lastMove: ["e2", "e4"] });
    const evMarks = v.marks.filter((m) => m.role !== "last-move");
    expect(evMarks.every((m) => m.square === "d7" || m.square === "b5")).toBe(true);
    expect(evMarks.some((m) => m.role === "alert-target")).toBe(true);
    // e8→d7 kept (touches d7); arrows not touching the set would be dropped.
    expect(v.shapes.map((s) => `${s.orig}|${s.dest}`).sort()).toEqual([
      "b5|d7",
      "d1|d7",
      "e8|d7",
    ]);
    // last-move survives isolation — it is position state, not evidence
    expect(v.marks.filter((m) => m.role === "last-move")).toHaveLength(2);
  });
});

describe("treatment/theme independence", () => {
  it("produces byte-identical output for walnut vs instrument and dark vs light", () => {
    // The module deliberately takes no treatment/theme input: for each of the
    // four render contexts the board passes the same (evidence, options), so
    // the serialized output must be identical — only CSS differs.
    const renderContexts = [
      ["walnut", "dark"],
      ["walnut", "light"],
      ["instrument", "dark"],
      ["instrument", "light"],
    ];
    const outputs = renderContexts.map(() =>
      JSON.stringify(
        evidenceView(ev, { intensity: 0.7, lastMove: ["e2", "e4"], selected: "d7" }),
      ),
    );
    expect(new Set(outputs).size).toBe(1);
    // And the hues have a single unthemed definition.
    expect(EVIDENCE_COLORS.alert.line).toBe("#e05c4b");
    expect(EVIDENCE_COLORS.attacker.line).toBe("#e8a13c");
    expect(EVIDENCE_COLORS.defender.line).toBe("#4f9ad8");
    expect(EVIDENCE_COLORS.imbalance.line).toBe("#5fb08a");
    expect(EVIDENCE_COLORS.key.line).toBe("#a98bd4");
  });
});

describe("arrow polygon geometry", () => {
  // 656 board → cell 82, u = 0.82. Values cross-checked against the
  // reference renderer's math (design/handoff-1/reference/Board.dc.html).
  it("computes the exact filled polygon for e5→c6 at cell 82", () => {
    expect(arrowPolygonPoints("e5", "c6", 82, "white")).toBe(
      "346.7,271.1 250.9,223.2 255.2,214.5 229.2,217.1 242.8,239.5 247.1,230.8 342.9,278.7",
    );
  });

  it("mirrors through the black orientation like the mark grid", () => {
    expect(squareCenter("e5", 82, "white")).toEqual([369, 287]);
    expect(squareCenter("e5", 82, "black")).toEqual([656 - 369, 656 - 287]);
    const white = arrowPolygonPoints("e5", "c6", 82, "white")
      .split(" ")
      .map((p) => p.split(",").map(Number));
    const black = arrowPolygonPoints("e5", "c6", 82, "black")
      .split(" ")
      .map((p) => p.split(",").map(Number));
    expect(black).toHaveLength(white.length);
    for (let i = 0; i < white.length; i++) {
      // Point-for-point 180° rotation about the board centre (656/2);
      // precision 0 → tolerance 0.5, covering the one-decimal rounding.
      expect(black[i][0]).toBeCloseTo(656 - white[i][0], 0);
      expect(black[i][1]).toBeCloseTo(656 - white[i][1], 0);
    }
  });

  it("offsets 33u from both centres along the arrow axis", () => {
    // e5 centre (369,287) → c6 centre (205,205); start/tip offsets 33u = 27.06.
    const pts = arrowPolygonPoints("e5", "c6", 82, "white")
      .split(" ")
      .map((p) => p.split(",").map(Number));
    const tip = pts[3]; // [ex, ey]
    expect(Math.hypot(tip[0] - 205, tip[1] - 205)).toBeCloseTo(27.06, 1);
    const startMid = [(pts[0][0] + pts[6][0]) / 2, (pts[0][1] + pts[6][1]) / 2];
    expect(Math.hypot(startMid[0] - 369, startMid[1] - 287)).toBeCloseTo(27.06, 1);
    // Shaft width = 2 × 5.2u across the start edge.
    expect(Math.hypot(pts[0][0] - pts[6][0], pts[0][1] - pts[6][1])).toBeCloseTo(2 * 5.2 * 0.82, 1);
  });

  it("computes the outline stroke width max(0.75, cell×0.016)", () => {
    expect(arrowStrokeWidth(82)).toBeCloseTo(1.312, 10);
    expect(arrowStrokeWidth(20)).toBe(0.75); // floor
  });
});

describe("board sizing", () => {
  it("snaps board sizes to multiples of 8", () => {
    expect(snapBoardSize(656)).toBe(656);
    expect(snapBoardSize(657)).toBe(656);
    expect(snapBoardSize(651)).toBe(648);
    expect(snapBoardSize(442)).toBe(440);
    expect(snapBoardSize(3)).toBe(8); // never below one cell per file
  });

  it("computes the README reference geometry at 656", () => {
    const walnut = boardGeometry(656, "walnut");
    expect(walnut.framePad).toBe(18); // round(656 × 0.028)
    expect(walnut.gutter).toBe(34); // round(656 × 0.052)
    expect(walnut.cell).toBe(82);
    const instrument = boardGeometry(656, "instrument");
    expect(instrument.framePad).toBe(0);
    expect(instrument.gutter).toBe(25); // round(656 × 0.038)
  });
});
