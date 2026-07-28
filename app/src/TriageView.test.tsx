// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TriageLists } from "./TriageView";
import type { ColorTriage, TriageItem } from "./lib/triage";

function item(over: Partial<TriageItem>): TriageItem {
  return {
    fen: "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
    ply: 2,
    games: 1,
    line: "1. e4 c5",
    eco: null,
    openingName: null,
    expectedSan: null,
    playedSan: null,
    opponentSan: null,
    hasExtension: false,
    examples: [],
    ...over,
  };
}

const CT: ColorTriage = {
  color: "white",
  hasCards: true,
  gamesScanned: 5,
  deviations: [
    item({
      fen: "dev-fen",
      line: "1. e4 e5 2. Nf3 Nc6",
      expectedSan: "Bb5",
      playedSan: "Bc4",
      ply: 5,
    }),
  ],
  gaps: [
    item({ fen: "gap-c5", opponentSan: "c5", games: 2 }),
    item({ fen: "gap-e6", opponentSan: "e6", line: "1. e4 e6" }),
  ],
  frontiers: [
    item({
      fen: "frontier-fen",
      line: "1. e4 e5 2. Nf3 Nc6 3. Bb5",
      ply: 5,
      hasExtension: true,
    }),
  ],
};

afterEach(cleanup);

describe("TriageLists", () => {
  it("renders the three ranked sections with honest captions and counts", () => {
    const { container } = render(
      <TriageLists ct={CT} selectedFen={null} onSelect={vi.fn()} />,
    );
    const titles = [...container.querySelectorAll(".triage-strip-title")].map(
      (el) => el.textContent,
    );
    expect(titles).toEqual([
      "DEVIATIONS — YOU LEFT YOUR OWN BOOK",
      "GAPS — OPPONENT MOVES YOUR BOOK DOESN'T ANSWER",
      "FRONTIERS — WHERE YOUR BOOK ENDS",
    ]);
    expect(container.textContent).toContain("book: Bb5 — played Bc4");
    expect(container.textContent).toContain("opponent played c5 — no card after it");
    expect(container.textContent).toContain("your book ends here");
    // Frequency badge and rank order.
    const gapRows = [...container.querySelectorAll(".triage-row")].filter((r) =>
      r.textContent?.includes("opponent played"),
    );
    expect(gapRows[0].textContent).toContain("2×");
    expect(gapRows[0].querySelector(".triage-rank")?.textContent).toBe("01");
    // Completed extensions are flagged in the row.
    expect(container.textContent).toContain("engine lines ready");
  });

  it("marks the selected row and reports clicks with kind + item", () => {
    const onSelect = vi.fn();
    const { container, getByText } = render(
      <TriageLists ct={CT} selectedFen="gap-c5" onSelect={onSelect} />,
    );
    const sel = container.querySelector(".triage-row.sel");
    expect(sel?.textContent).toContain("opponent played c5");
    fireEvent.click(getByText("book: Bb5 — played Bc4"));
    expect(onSelect).toHaveBeenCalledWith(
      "deviation",
      expect.objectContaining({ fen: "dev-fen", expectedSan: "Bb5" }),
    );
  });

  it("says 'none found' for an empty class instead of hiding it", () => {
    const empty: ColorTriage = { ...CT, deviations: [], gaps: [], frontiers: [] };
    const { container } = render(
      <TriageLists ct={empty} selectedFen={null} onSelect={vi.fn()} />,
    );
    expect(container.querySelectorAll(".triage-none")).toHaveLength(3);
  });
});
