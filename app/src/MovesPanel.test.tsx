// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import MovesPanel from "./MovesPanel";
import { movesRowsFromSans, type MovesRow } from "./lib/movesView";
import type { AnnotationMode } from "./lib/gameView";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const ROWS = movesRowsFromSans(["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"], START);

function panel(currentPly: number, annotationMode: AnnotationMode = "full") {
  return (
    <MovesPanel
      rows={ROWS}
      currentPly={currentPly}
      evals={null}
      annotationMode={annotationMode}
      onAnnotationMode={() => {}}
      onSelectPly={() => {}}
      editing={null}
    />
  );
}

/** jsdom has no scrollIntoView; its zeroed layout rects make the
 * "current move out of view" branch always taken, so a spy observes
 * exactly when the follow-scroll effect fires. */
const scrollSpy = vi.fn();

beforeEach(() => {
  scrollSpy.mockClear();
  Element.prototype.scrollIntoView = scrollSpy;
});
afterEach(cleanup);

describe("MovesPanel follow-scroll (audit #14)", () => {
  it("centers the current move when the rows first arrive (game load)", () => {
    // A resumed game renders mid-line on MOUNT — not only on stepping.
    render(panel(4));
    expect(scrollSpy).toHaveBeenCalledTimes(1);
    expect(scrollSpy).toHaveBeenCalledWith({ block: "center" });
  });

  it("re-centers after the annotation display mode changes", () => {
    const { rerender } = render(panel(4, "full"));
    scrollSpy.mockClear();
    // Toggling full → hidden reflows the list; the effect must re-run.
    rerender(panel(4, "hidden"));
    expect(scrollSpy).toHaveBeenCalledTimes(1);
    // A no-op rerender does not scroll again (free scrolling stays free).
    scrollSpy.mockClear();
    rerender(panel(4, "hidden"));
    expect(scrollSpy).not.toHaveBeenCalled();
  });

  it("still follows stepping, and ply 0 with no current cell scrolls to top", () => {
    const { rerender } = render(panel(2));
    scrollSpy.mockClear();
    rerender(panel(3));
    expect(scrollSpy).toHaveBeenCalledTimes(1);
    scrollSpy.mockClear();
    rerender(panel(0)); // before the first move: nothing to center
    expect(scrollSpy).not.toHaveBeenCalled();
  });
});

describe("COACH narration rows are first-class (run 10 unification)", () => {
  // Two narrated plies so the current-ply gating is observable.
  const NARRATED: MovesRow[] = [
    ...movesRowsFromSans(["e4", "e5"], START),
    { kind: "narration", ply: 1, text: "The center is claimed." },
    { kind: "narration", ply: 2, text: "Black answers in kind." },
  ];

  function narratedPanel(
    currentPly: number,
    onSelectPly: (ply: number) => void,
    onNarrationHover?: (h: boolean) => void,
  ) {
    return (
      <MovesPanel
        rows={NARRATED}
        currentPly={currentPly}
        evals={null}
        annotationMode="full"
        onAnnotationMode={() => {}}
        onSelectPly={onSelectPly}
        editing={null}
        onNarrationHover={onNarrationHover}
      />
    );
  }

  it("clicking a COACH row navigates to its ply, like clicking the move", () => {
    const onSelectPly = vi.fn();
    render(narratedPanel(1, onSelectPly));
    fireEvent.click(screen.getByText("Black answers in kind."));
    expect(onSelectPly).toHaveBeenCalledWith(2);
  });

  it("hover fires the evidence callback ONLY on the current ply's row", () => {
    const hover = vi.fn();
    render(narratedPanel(1, () => {}, hover));
    const current = screen.getByText("The center is claimed.").closest("button")!;
    fireEvent.mouseEnter(current);
    expect(hover).toHaveBeenLastCalledWith(true);
    fireEvent.mouseLeave(current);
    expect(hover).toHaveBeenLastCalledWith(false);
    expect(current.className).toContain("evidence");

    hover.mockClear();
    const other = screen.getByText("Black answers in kind.").closest("button")!;
    fireEvent.mouseEnter(other);
    fireEvent.mouseLeave(other);
    expect(hover).not.toHaveBeenCalled();
    expect(other.className).not.toContain("evidence");
  });

  it("without the hover callback (Explain off / preview) rows still navigate", () => {
    const onSelectPly = vi.fn();
    render(narratedPanel(1, onSelectPly, undefined));
    const row = screen.getByText("The center is claimed.").closest("button")!;
    fireEvent.mouseEnter(row); // no callback — must not throw
    fireEvent.click(row);
    expect(onSelectPly).toHaveBeenCalledWith(1);
    expect(row.className).not.toContain("evidence");
  });
});
