// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GradeRow } from "./TrainView";
import type { DueCard } from "./lib/db";

/** Fixture card with scheduler previews in RAW days (the wire format). */
const CARD: DueCard = {
  cardId: 7,
  repertoireName: "main (white)",
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  expectedSan: "e4",
  expectedUci: "e2e4",
  ply: 0,
  linePrefix: "",
  due: "2026-07-26 00:00:00",
  isNew: false,
  reps: 3,
  lapses: 2,
  previews: { again: 0.4, hard: 2.2, good: 9.4, easy: 21.0 },
};

afterEach(cleanup);

describe("GradeRow (design/handoff-2 §Openings SRS grade row)", () => {
  it("shows the four grades with their REAL formatted preview intervals", () => {
    const { container } = render(<GradeRow card={CARD} onGrade={vi.fn()} />);
    const buttons = [...container.querySelectorAll("button.grade-btn")];
    expect(buttons).toHaveLength(4);
    const text = (b: Element, sel: string) => b.querySelector(sel)?.textContent;
    // lib/train.ts formatInterval over the fixture previews.
    expect(buttons.map((b) => text(b, ".grade-label"))).toEqual(["Again", "Hard", "Good", "Easy"]);
    expect(buttons.map((b) => text(b, ".grade-key"))).toEqual(["1", "2", "3", "4"]);
    expect(buttons.map((b) => text(b, ".grade-next"))).toEqual(["<1d", "2d", "9d", "21d"]);
  });

  it("colours the buttons bad / dim / good / info", () => {
    const { container } = render(<GradeRow card={CARD} onGrade={vi.fn()} />);
    const classes = [...container.querySelectorAll("button.grade-btn")].map((b) => b.className);
    expect(classes).toEqual([
      "grade-btn bad",
      "grade-btn dim",
      "grade-btn good",
      "grade-btn info",
    ]);
  });

  it("reports the clicked grade and honours disabled (pre-reveal)", () => {
    const onGrade = vi.fn();
    const { container, rerender } = render(<GradeRow card={CARD} onGrade={onGrade} />);
    const buttons = () => [...container.querySelectorAll("button.grade-btn")];
    fireEvent.click(buttons()[2]);
    expect(onGrade).toHaveBeenCalledWith("good");
    rerender(<GradeRow card={CARD} onGrade={onGrade} disabled />);
    fireEvent.click(buttons()[0]);
    expect(onGrade).toHaveBeenCalledTimes(1);
  });
});
