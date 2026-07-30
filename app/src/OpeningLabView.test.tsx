// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BranchList } from "./OpeningLabView";
import type { LabNode } from "./lib/openingLab";

function node(over: Partial<LabNode>): LabNode {
  return {
    fen: "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    ply: 5,
    line: "1. e4 e5 2. Nf3 Nc6",
    games: 3,
    eco: "C44",
    openingName: "King's Knight Opening",
    repSan: null,
    hasExtension: false,
    damage: 0.5,
    moves: [
      {
        san: "Bc4",
        games: 1,
        scorePct: 0,
        avgEvalCp: null,
        evalGames: 0,
        inBook: true,
        inRep: false,
        damage: 0.5,
        replies: [],
      },
      {
        san: "Bb5",
        games: 2,
        scorePct: 50,
        avgEvalCp: -20,
        evalGames: 1,
        inBook: true,
        inRep: false,
        damage: 0,
        replies: [],
      },
    ],
    examples: [],
    ...over,
  };
}

afterEach(cleanup);

describe("BranchList", () => {
  it("renders damage-ranked rows with move distribution, book move and badges", () => {
    const nodes = [
      node({ repSan: "Bb5", hasExtension: true }),
      node({ fen: "second", line: "1. e4 e5", ply: 3, damage: 0, moves: [] }),
    ];
    const { container } = render(
      <BranchList nodes={nodes} selectedFen={null} onSelect={vi.fn()} />,
    );
    const rows = [...container.querySelectorAll(".triage-row")];
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector(".triage-rank")?.textContent).toBe("01");
    expect(rows[0].textContent).toContain("move 3 · Bc4 1× (0%) · Bb5 2× (50%)");
    expect(rows[0].textContent).toContain("book: Bb5");
    expect(rows[0].textContent).toContain("engine lines ready");
    expect(rows[0].textContent).toContain("dmg 0.5");
    // The second row carries neither badge — no fake affordances.
    expect(rows[1].textContent).not.toContain("book:");
    expect(rows[1].textContent).not.toContain("engine lines ready");
  });

  it("marks the selected row and reports clicks with the node", () => {
    const onSelect = vi.fn();
    const nodes = [node({}), node({ fen: "second", line: "1. e4 e5" })];
    const { container } = render(
      <BranchList nodes={nodes} selectedFen="second" onSelect={onSelect} />,
    );
    expect(container.querySelector(".triage-row.sel .triage-rank")?.textContent).toBe("02");
    fireEvent.click(container.querySelectorAll(".triage-row")[0]);
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ line: "1. e4 e5 2. Nf3 Nc6" }),
    );
  });

  it("says so when no branch points exist instead of rendering nothing", () => {
    const { container } = render(<BranchList nodes={[]} selectedFen={null} onSelect={vi.fn()} />);
    expect(container.querySelector(".triage-none")?.textContent).toBe(
      "no in-book branch points found",
    );
  });
});
