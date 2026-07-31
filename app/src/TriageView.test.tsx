// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import TriageView, { TriageLists } from "./TriageView";
import {
  triageInferRepertoire,
  triageReport,
  type ColorTriage,
  type InferredRepertoire,
  type TriageItem,
  type TriageReport,
} from "./lib/triage";
import { trainAddLine } from "./lib/db";

vi.mock("./Board", () => ({
  default: () => <div data-testid="board" />,
}));

// Explicit export list: extend it when TriageView imports more from db.
vi.mock("./lib/db", () => ({
  matchingPlayers: vi.fn(() => Promise.resolve([])),
  selfPlayerGet: vi.fn(() => Promise.resolve(null)),
  selfPlayerSet: vi.fn(() => Promise.resolve()),
  trainAddLine: vi.fn(() =>
    Promise.resolve({ repertoire: "main (white)", cardsAdded: 4, cardsExisting: 1 }),
  ),
  identityGroup: vi.fn(() =>
    Promise.resolve([
      { playerId: 1, name: "Infer, Ida", games: 0 },
      { playerId: 2, name: "Ida Infer", games: 0 },
    ]),
  ),
}));

// Keep the pure helpers real; stub only the IPC wrappers.
vi.mock("./lib/triage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/triage")>();
  return {
    ...actual,
    triageReport: vi.fn(),
    triageInferRepertoire: vi.fn(),
    triageExtend: vi.fn(),
    triageExtensionStatus: vi.fn(() =>
      Promise.resolve({ extension: null, jobStatus: null, jobsAhead: 0, workerActive: false }),
    ),
  };
});

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
  gamesSeen: 5,
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
beforeEach(() => {
  localStorage.clear();
});

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

/* ------------------------------------------------------------------ */
/* Full-view flows: default color tab + the card-less suggestion flow */
/* ------------------------------------------------------------------ */

const ctOf = (over: Partial<ColorTriage>): ColorTriage => ({
  color: "white",
  hasCards: false,
  gamesScanned: 0,
  gamesSeen: 0,
  deviations: [],
  gaps: [],
  frontiers: [],
  ...over,
});

const reportOf = (white: Partial<ColorTriage>, black: Partial<ColorTriage>): TriageReport => ({
  player: "Infer, Ida",
  white: ctOf(white),
  black: ctOf({ color: "black", ...black }),
});

const emptyInference = (gamesScanned: number): InferredRepertoire => ({
  player: "Infer, Ida",
  color: "white",
  gamesScanned,
  lines: [],
});

function renderAndRun() {
  const utils = render(<TriageView onOpenGameAt={vi.fn()} />);
  const input = utils.container.querySelector("input")!;
  fireEvent.change(input, { target: { value: "Infer, Ida" } });
  fireEvent.click(utils.getByText("Run triage"));
  return utils;
}

describe("TriageView — default color tab", () => {
  it("opens on the color that has cards, never a dead White tab", async () => {
    vi.mocked(triageReport).mockResolvedValue(
      reportOf(
        { gamesSeen: 3 },
        {
          hasCards: true,
          gamesScanned: 8,
          gamesSeen: 8,
          gaps: [item({ opponentSan: "d4" })],
        },
      ),
    );
    vi.mocked(triageInferRepertoire).mockResolvedValue(emptyInference(3));
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(getByText("as Black").className).toBe("cur"));
    expect(container.textContent).toContain("opponent played d4");
  });

  it("with no cards anywhere, opens on the color with more games", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 1 }, { gamesSeen: 6 }));
    vi.mocked(triageInferRepertoire).mockResolvedValue({
      ...emptyInference(6),
      color: "black",
    });
    const { getByText } = renderAndRun();
    await waitFor(() => expect(getByText("as Black").className).toBe("cur"));
    expect(vi.mocked(triageInferRepertoire)).toHaveBeenCalledWith("Infer, Ida", "black");
  });
});

describe("TriageView — card-less color suggestion flow", () => {
  const INFERENCE: InferredRepertoire = {
    player: "Infer, Ida",
    color: "white",
    gamesScanned: 5,
    lines: [
      {
        sans: ["e4", "c5", "Nf3"],
        games: 4,
        score: 62.5,
        eco: "B27",
        openingName: "Sicilian Defense",
      },
      { sans: ["e4", "e5"], games: 3, score: 50, eco: "C20", openingName: "King's Pawn Game" },
    ],
  };

  it("suggests the inferred lines instead of a dead end, honestly captioned", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 5 }, { gamesSeen: 1 }));
    vi.mocked(triageInferRepertoire).mockResolvedValue(INFERENCE);
    const { container } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain(
        "No White repertoire yet — but your games already show what you play:",
      ),
    );
    // The old misleading summary never renders for a skipped color.
    expect(container.textContent).not.toContain("no triage points");
    expect(container.textContent).toContain(
      "White games are skipped until a White repertoire exists — adopt one below.",
    );
    // Numbered SAN, game support, score and dataset name per line.
    expect(container.textContent).toContain("1. e4 c5 2. Nf3");
    expect(container.textContent).toContain("4 games · 62.5% score · B27 Sicilian Defense");
    expect(container.textContent).toContain("3 games · 50% score · C20 King's Pawn Game");
  });

  it("adopt-all adds every line, reports real card totals, and re-runs the triage", async () => {
    vi.mocked(triageReport)
      .mockResolvedValueOnce(reportOf({ gamesSeen: 5 }, { gamesSeen: 1 }))
      .mockResolvedValue(
        reportOf(
          { hasCards: true, gamesScanned: 5, gamesSeen: 5, frontiers: [item({})] },
          { gamesSeen: 1 },
        ),
      );
    vi.mocked(triageInferRepertoire).mockResolvedValue(INFERENCE);
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("Adopt all 2 lines"));
    fireEvent.click(getByText("Adopt all 2 lines"));
    await waitFor(() => expect(vi.mocked(trainAddLine)).toHaveBeenCalledTimes(2));
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith("white", ["e4", "c5", "Nf3"]);
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith("white", ["e4", "e5"]);
    // Real totals from the trainAddLine results (4 + 4 new, 1 + 1 existing).
    await waitFor(() =>
      expect(container.textContent).toContain(
        'Adopted 2 lines into "main (white)": 8 new cards, 2 positions already covered.',
      ),
    );
    // The automatic re-run lands on actual triage points.
    await waitFor(() => expect(container.textContent).toContain("your book ends here"));
    expect(container.textContent).not.toContain("No White repertoire yet");
  });

  it("shows an honest busy state, then the no-supported-lines message", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 4 }, {}));
    let resolve!: (v: InferredRepertoire) => void;
    vi.mocked(triageInferRepertoire).mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const { container } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain(
        "Reading your White games for the lines you already play…",
      ),
    );
    resolve(emptyInference(4));
    await waitFor(() =>
      expect(container.textContent).toContain(
        "Walked 4 White games, but no opening line repeats",
      ),
    );
  });

  it("with zero games for the identity, names the forms searched and points at Profile", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({}, {}));
    vi.mocked(triageInferRepertoire).mockResolvedValue(emptyInference(0));
    const { container } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain("No White games found for this identity"),
    );
    expect(container.textContent).toContain("searched: Infer, Ida, Ida Infer");
    expect(container.textContent).toContain("Profile screen’s INCLUDES strip");
  });
});
