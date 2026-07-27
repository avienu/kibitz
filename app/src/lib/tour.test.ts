import { describe, expect, it } from "vitest";
import { TOUR_STEPS, initialTour, reduceTour, tourCounter } from "./tour";

describe("first-run tour state machine (design/handoff-2 §Help & tour)", () => {
  it("has one card per rail region: header, four groups, footer", () => {
    expect(TOUR_STEPS.map((s) => s.anchor)).toEqual([
      "header",
      "study",
      "coach",
      "train",
      "data",
      "footer",
    ]);
  });

  it("next walks every card in order and finishes past the last", () => {
    let s = initialTour();
    for (let i = 0; i < TOUR_STEPS.length - 1; i++) {
      s = reduceTour(s, { type: "next" });
      expect(s).toEqual({ step: i + 1, done: false });
    }
    s = reduceTour(s, { type: "next" });
    expect(s.done).toBe(true);
  });

  it("skip ends immediately from any card", () => {
    const mid = reduceTour(initialTour(), { type: "next" });
    expect(reduceTour(mid, { type: "skip" }).done).toBe(true);
    expect(reduceTour(initialTour(), { type: "skip" }).done).toBe(true);
  });

  it("replay restarts from the first card after finishing", () => {
    let s = initialTour();
    s = reduceTour(s, { type: "skip" });
    expect(s.done).toBe(true);
    s = reduceTour(s, { type: "replay" });
    expect(s).toEqual({ step: 0, done: false });
  });

  it("counter renders 1-based over the total", () => {
    expect(tourCounter(0)).toBe(`1 / ${TOUR_STEPS.length}`);
    expect(tourCounter(1, 6)).toBe("2 / 6");
  });
});
