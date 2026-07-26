import { describe, expect, it } from "vitest";
import type { AnalysisRow } from "./analyses";
import { DEFAULT_INTENSITY } from "./evidence";
import {
  blockReferencesSquare,
  deriveEvidence,
  deriveIntensity,
  evalBarView,
  evalSourceLabel,
  fitBoardSize,
  isEditableTarget,
  keyboardAction,
  MIN_BOARD_SIZE,
  normalizeEvidence,
  railCollapsed,
  reduceGameView,
  selectionNote,
  selectPlyAnalysis,
  sentenceOpacity,
  unionEvidence,
  type ExplanationBlockJson,
  type ExplanationJson,
  type GameViewState,
} from "./gameView";

const block = (over: Partial<ExplanationBlockJson> = {}): ExplanationBlockJson => ({
  kind: "alert",
  text: { coach: "c", neutral: "n" },
  evidence: {},
  ...over,
});

const explanation = (blocks: ExplanationBlockJson[]): ExplanationJson => ({
  schema_version: 3,
  tag: "TACTICAL SCREEN FIRED",
  headline: { coach: "h", neutral: "h" },
  blocks,
});

describe("evidence derivation (README §State Management)", () => {
  const b0 = block({ evidence: { alerts: ["d7"], attackers: ["b5"], arrows: [{ from: "b5", to: "d7", kind: "attacker" }] } });
  const b1 = block({ kind: "imbalance", evidence: { imbalance: ["e8", "d7"] } });
  const expl = explanation([b0, b1]);

  it("normalizes omitted wire arrays (serde skips empty vecs)", () => {
    const e = normalizeEvidence({ alerts: ["a1"] });
    expect(e.alerts).toEqual(["a1"]);
    expect(e.attackers).toEqual([]);
    expect(e.arrows).toEqual([]);
    expect(normalizeEvidence(null).key).toEqual([]);
  });

  it("no hover: union of all blocks", () => {
    const e = deriveEvidence(expl, null)!;
    expect(e.alerts).toEqual(["d7"]);
    expect(e.attackers).toEqual(["b5"]);
    expect(e.imbalance).toEqual(["e8", "d7"]);
    expect(e.arrows).toHaveLength(1);
  });

  it("hover: only the hovered sentence's evidence", () => {
    const e = deriveEvidence(expl, 1)!;
    expect(e.alerts).toEqual([]);
    expect(e.imbalance).toEqual(["e8", "d7"]);
    expect(e.arrows).toEqual([]);
  });

  it("hover out of range falls back to the union", () => {
    expect(deriveEvidence(expl, 99)!.alerts).toEqual(["d7"]);
  });

  it("null explanation derives null; unionEvidence flattens", () => {
    expect(deriveEvidence(null, null)).toBeNull();
    expect(unionEvidence([b0, b1]).imbalance).toEqual(["e8", "d7"]);
  });

  it("intensity: 1.0 hovered, 0.44 baseline", () => {
    expect(deriveIntensity(0)).toBe(1);
    expect(deriveIntensity(null)).toBe(DEFAULT_INTENSITY);
    expect(DEFAULT_INTENSITY).toBe(0.44);
  });
});

describe("prose filtering by selected square", () => {
  const referencing = block({ evidence: { arrows: [{ from: "b5", to: "d7", kind: "attacker" }] } });
  const other = block({ evidence: { imbalance: ["a1"] } });

  it("references via any evidence array or arrow endpoint", () => {
    expect(blockReferencesSquare(referencing, "d7")).toBe(true);
    expect(blockReferencesSquare(referencing, "b5")).toBe(true);
    expect(blockReferencesSquare(other, "d7")).toBe(false);
    expect(blockReferencesSquare(other, "a1")).toBe(true);
  });

  it("non-referencing sentences drop to 0.34; no selection = full", () => {
    expect(sentenceOpacity(other, "d7")).toBe(0.34);
    expect(sentenceOpacity(referencing, "d7")).toBe(1);
    expect(sentenceOpacity(other, null)).toBe(1);
  });

  it("footer meta reflects the selection", () => {
    expect(selectionNote(null)).toBe("hover a line to isolate its evidence");
    expect(selectionNote("d7")).toBe("filtered to d7");
  });
});

describe("reduceGameView", () => {
  const base: GameViewState = {
    ply: 10,
    hoverSentence: 1,
    selectedSquare: "d7",
    voice: "coach",
    annotationMode: "full",
    boardTreatment: "walnut",
    theme: "dark",
    flipped: false,
  };

  it("stepping clears hover and selection and clamps", () => {
    const s = reduceGameView(base, { type: "step", delta: 5, plyCount: 33 });
    expect(s.ply).toBe(15);
    expect(s.hoverSentence).toBeNull();
    expect(s.selectedSquare).toBeNull();
    expect(reduceGameView(base, { type: "step", delta: -99, plyCount: 33 }).ply).toBe(0);
    expect(reduceGameView(base, { type: "step", delta: 99, plyCount: 33 }).ply).toBe(33);
  });

  it("setPly clears transient state too", () => {
    const s = reduceGameView(base, { type: "setPly", ply: 3, plyCount: 33 });
    expect(s).toMatchObject({ ply: 3, hoverSentence: null, selectedSquare: null });
  });

  it("square selection toggles on repeat click", () => {
    const cleared = reduceGameView(base, { type: "selectSquare", square: "d7" });
    expect(cleared.selectedSquare).toBeNull();
    const moved = reduceGameView(base, { type: "selectSquare", square: "e4" });
    expect(moved.selectedSquare).toBe("e4");
  });

  it("flip toggles; voice/mode/theme/treatment set", () => {
    expect(reduceGameView(base, { type: "toggleFlip" }).flipped).toBe(true);
    expect(reduceGameView(base, { type: "setVoice", voice: "neutral" }).voice).toBe("neutral");
    expect(reduceGameView(base, { type: "setAnnotationMode", mode: "hover" }).annotationMode).toBe(
      "hover",
    );
    expect(reduceGameView(base, { type: "setTheme", theme: "light" }).theme).toBe("light");
    expect(
      reduceGameView(base, { type: "setTreatment", treatment: "instrument" }).boardTreatment,
    ).toBe("instrument");
  });
});

describe("keyboard map (README §Interactions)", () => {
  it("maps the documented keys", () => {
    expect(keyboardAction("ArrowRight")).toBe("next");
    expect(keyboardAction("ArrowLeft")).toBe("prev");
    expect(keyboardAction("ArrowDown")).toBe("fwd5");
    expect(keyboardAction("ArrowUp")).toBe("back5");
    expect(keyboardAction("Home")).toBe("start");
    expect(keyboardAction("End")).toBe("end");
    expect(keyboardAction("f")).toBe("flip");
    expect(keyboardAction("F")).toBe("flip");
    expect(keyboardAction("e")).toBe("explain");
    expect(keyboardAction("x")).toBeNull();
  });

  it("never fires while a text input is focused or a modifier is held", () => {
    expect(keyboardAction("ArrowRight", { editable: true })).toBeNull();
    expect(keyboardAction("f", { editable: true })).toBeNull();
    expect(keyboardAction("e", { modifier: true })).toBeNull();
  });

  it("detects editable targets incl. contenteditable", () => {
    expect(isEditableTarget({ tagName: "TEXTAREA" })).toBe(true);
    expect(isEditableTarget({ tagName: "INPUT" })).toBe(true);
    expect(isEditableTarget({ tagName: "SELECT" })).toBe(true);
    expect(isEditableTarget({ tagName: "DIV", isContentEditable: true })).toBe(true);
    expect(isEditableTarget({ tagName: "BUTTON" })).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

describe("eval bar state derivation (deliverable 2a)", () => {
  const row = (over: Partial<AnalysisRow>): AnalysisRow => ({
    ply: 4,
    kind: "fresh",
    engine: "Stockfish 18",
    depth: 24,
    nodes: null,
    evalCp: 60,
    createdAt: "2026-01-01",
    ...over,
  });

  it("no analysis: empty track, muted dash, never a fake 0.0", () => {
    const v = evalBarView(null);
    expect(v).toEqual({ state: "no-data", fillPct: null, readout: "—", tooltip: "no analysis" });
  });

  it("cp: fill = clamp(6, 50 + pawns×9, 94), readout ±N.N", () => {
    const v = evalBarView(row({ evalCp: 260, ply: 4 })); // +2.6 White POV (even ply)
    expect(v.state).toBe("cp");
    expect(v.fillPct).toBeCloseTo(50 + 2.6 * 9);
    expect(v.readout).toBe("+2.6");
    const big = evalBarView(row({ evalCp: 2000 }));
    expect(big.fillPct).toBe(94);
    const lost = evalBarView(row({ evalCp: -2000 }));
    expect(lost.fillPct).toBe(6);
    expect(lost.readout).toBe("-20.0");
  });

  it("fresh rows are side-to-move POV: negated at odd plies", () => {
    const v = evalBarView(row({ evalCp: 100, ply: 5 }));
    expect(v.readout).toBe("-1.0");
  });

  it("mate sentinel (±10000 fresh cp) pins the bar to the winner", () => {
    const w = evalBarView(row({ evalCp: 10_000, ply: 4 }));
    expect(w).toMatchObject({ state: "mate", fillPct: 94, winner: "white", readout: "#" });
    const b = evalBarView(row({ evalCp: 10_000, ply: 5 })); // stm POV → Black winning
    expect(b).toMatchObject({ state: "mate", fillPct: 6, winner: "black" });
  });

  it("explicit mate distance (from an explanation readout) shows #N", () => {
    const v = evalBarView(row({}), -3);
    expect(v).toMatchObject({ state: "mate", fillPct: 6, winner: "black", readout: "#3" });
  });

  it("fresh beats legacy at the same ply; first row wins within a kind", () => {
    const rows: AnalysisRow[] = [
      row({ kind: "legacy-import", engine: "Deep Rybka 4 2011", evalCp: -300 }),
      row({ kind: "fresh", evalCp: 40 }),
      row({ kind: "fresh", evalCp: 99 }),
      row({ ply: 7, kind: "legacy-import", engine: "Deep Rybka 4 2011" }),
    ];
    const picked = selectPlyAnalysis(rows, 4)!;
    expect(picked.kind).toBe("fresh");
    expect(picked.evalCp).toBe(40);
    expect(selectPlyAnalysis(rows, 7)!.kind).toBe("legacy-import");
    expect(selectPlyAnalysis(rows, 99)).toBeNull();
  });

  it("tooltips name the source", () => {
    expect(evalSourceLabel(row({}))).toBe("Stockfish 18 · depth 24 (fresh)");
    expect(evalSourceLabel(row({ depth: null, nodes: 2_000_000 }))).toBe(
      "Stockfish 18 · nodes 2,000,000 (fresh)",
    );
    expect(evalSourceLabel(row({ kind: "legacy-import", engine: "Deep Rybka 4 2011" }))).toBe(
      "legacy import · Deep Rybka 4 2011",
    );
  });
});

describe("resize snapping (deliverable 2c)", () => {
  it("board snaps to the largest multiple of 8 whose chrome fits", () => {
    const s = fitBoardSize(760, 900, "walnut");
    expect(s % 8).toBe(0);
    // Walnut chrome ≈ size×0.108; the snapped board + chrome must fit.
    expect(s + Math.round(s * 0.028) * 2 + Math.round(s * 0.052)).toBeLessThanOrEqual(760);
    // And the next size up must NOT fit.
    const n = s + 8;
    expect(n + Math.round(n * 0.028) * 2 + Math.round(n * 0.052)).toBeGreaterThan(760);
  });

  it("656 design size fits its design column", () => {
    // 656 + 2×18 + 34 = 726 of chrome+grid.
    expect(fitBoardSize(726, 900, "walnut")).toBe(656);
  });

  it("never shrinks below the 496 minimum", () => {
    expect(fitBoardSize(200, 200, "walnut")).toBe(MIN_BOARD_SIZE);
  });

  it("height constrains too", () => {
    expect(fitBoardSize(4000, 726, "walnut")).toBe(656);
  });

  it("rail collapses below 1280", () => {
    expect(railCollapsed(1279)).toBe(true);
    expect(railCollapsed(1280)).toBe(false);
  });
});
