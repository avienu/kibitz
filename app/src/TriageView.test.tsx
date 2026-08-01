// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import TriageView, { TriageLists } from "./TriageView";
import {
  triageExtensionStatus,
  triageInferFrom,
  triageInferRepertoire,
  triageReport,
  type ColorTriage,
  type InferredRepertoire,
  type TriageItem,
  type TriageReport,
} from "./lib/triage";
import { trainAddLine } from "./lib/db";
import { gameFromSans } from "./lib/game";

// The mock board exposes move-input triggers when the view makes it
// movable (the "I know my answer" flow) — the real legality/SAN wiring
// (trainDests, sanForBoardMove) still runs in the view. It also reflects
// the fen/lastMove props so the hover-scrub preview tests can see what
// position the aside board is showing.
vi.mock("./Board", () => ({
  default: ({
    fen,
    lastMove,
    movable,
  }: {
    fen: string;
    lastMove?: [string, string];
    movable?: { onMove: (o: string, d: string) => void };
  }) => (
    <div data-testid="board" data-fen={fen} data-lastmove={lastMove ? lastMove.join("") : ""}>
      {movable && (
        <>
          <button onClick={() => movable.onMove("g8", "f6")}>board-g8f6</button>
          <button onClick={() => movable.onMove("b8", "c6")}>board-b8c6</button>
        </>
      )}
    </div>
  ),
}));

// Explicit export list: extend it when TriageView imports more from db.
vi.mock("./lib/db", () => ({
  selfPlayerGet: vi.fn(() => Promise.resolve("Infer, Ida")),
  trainAddLine: vi.fn(() =>
    Promise.resolve({
      repertoire: "main (white)",
      cardsAdded: 4,
      cardsExisting: 1,
      cardsReplaced: 1,
    }),
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
    triageInferFrom: vi.fn(),
    triageExtend: vi.fn(),
    triageExtensionStatus: vi.fn(() =>
      Promise.resolve({
        extension: null,
        jobStatus: null,
        jobsAhead: 0,
        workerActive: false,
        search: null,
      }),
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
    playedCount: 0,
    cardFollowed: 0,
    realityCheck: false,
    inferredLines: [],
    wholeOpening: false,
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
    expect(container.textContent).toContain("your book ends after");
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
  // Identity is canonical (selfPlayerGet mock) and triage AUTO-RUNS on
  // mount — visiting the page IS running it (2026-07-30 ruling).
  return render(<TriageView onOpenGameAt={vi.fn()} />);
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
    // The inference call lands a microtask after the tab settles — wait
    // for it too (this raced on the slower CI runner).
    await waitFor(() =>
      expect(vi.mocked(triageInferRepertoire)).toHaveBeenCalledWith("Infer, Ida", "black"),
    );
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
    await waitFor(() => expect(container.textContent).toContain("your book ends after"));
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

/* ------------------------------------------------------------------ */
/* Declared-vs-played (2026-07-30 v2): reality panel, whole-opening    */
/* holes, and the board-played "I know my answer" flow                 */
/* ------------------------------------------------------------------ */

/** After 1.e4 — Black (the user) to move. */
const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
/** After 1.d4 — Black (the user) to move. */
const AFTER_D4 = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq - 0 1";

const REALITY = item({
  fen: AFTER_E4,
  ply: 2,
  games: 119,
  line: "1. e4",
  expectedSan: "e5",
  playedSan: "c5",
  playedCount: 119,
  cardFollowed: 1,
  realityCheck: true,
  inferredLines: [
    {
      sans: ["e4", "c5", "Nf3", "d6"],
      games: 63,
      score: 48.4,
      eco: "B50",
      openingName: "Sicilian Defense",
    },
    { sans: ["e4", "c5", "c3"], games: 21, score: 52.4, eco: "B22", openingName: "Alapin" },
  ],
});

const realityReport = (): TriageReport =>
  reportOf(
    { gamesSeen: 2 },
    { hasCards: true, gamesScanned: 199, gamesSeen: 199, deviations: [REALITY] },
  );

describe("TriageView — reality-check deviation panel", () => {
  it("confronts declared-vs-played instead of a scolding row, and adopts what you play", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(realityReport());
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain(
        "Your cards say 1... e5 — but you've played 1... c5 in 119 of 120 games. " +
          "That looks like your real repertoire.",
      ),
    );
    // The summary uses the new honest shape; the scolding row is gone
    // (the deviations section reports none found instead).
    expect(container.textContent).toContain("your play disagrees with your cards at 1 position");
    expect(container.textContent).not.toContain("book: e5 — played c5");
    // Inferred lines with games/score/name captions.
    expect(container.textContent).toContain("1. e4 c5 2. Nf3 d6");
    expect(container.textContent).toContain("63 games · 48.4% score · B50 Sicilian Defense");

    fireEvent.click(getByText("Adopt what you play"));
    await waitFor(() => expect(vi.mocked(trainAddLine)).toHaveBeenCalledTimes(2));
    // REPLACE mode: the conflicting card must be rewritten, or the panel
    // would return forever.
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith(
      "black",
      ["e4", "c5", "Nf3", "d6"],
      undefined,
      undefined,
      true,
    );
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith(
      "black",
      ["e4", "c5", "c3"],
      undefined,
      undefined,
      true,
    );
    await waitFor(() =>
      expect(container.textContent).toContain(
        'Adopted what you play into "main (white)": 8 new cards, ' +
          "2 cards rewritten to your move, 2 positions already covered.",
      ),
    );
    // The adoption re-runs the triage.
    await waitFor(() => expect(vi.mocked(triageReport)).toHaveBeenCalledTimes(2));
  });

  it("'Keep training the cards' dismisses the panel and lists the deviation normally", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(realityReport());
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("Your cards say 1... e5"));
    fireEvent.click(getByText("Keep training the cards"));
    expect(container.textContent).not.toContain("Your cards say 1... e5");
    expect(container.textContent).toContain("book: e5 — played c5");
    expect(vi.mocked(trainAddLine)).not.toHaveBeenCalled();
  });

  it("offers the board as a third path: play a THIRD move, confirm, replace", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(realityReport());
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("Your cards say 1... e5"));
    fireEvent.click(getByText("I know my answer — play it on the board"));
    // The aside board is now movable; play 1...Nc6 (neither e5 nor c5).
    fireEvent.click(getByText("board-b8c6"));
    expect(container.textContent).toContain("Set 1... Nc6 as your repertoire answer?");
    fireEvent.click(getByText("Set as my answer"));
    await waitFor(() =>
      expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith(
        "black",
        ["e4", "Nc6"],
        undefined,
        undefined,
        true,
      ),
    );
  });
});

const HOLE = item({
  fen: AFTER_D4,
  ply: 1,
  games: 63,
  line: "1. d4",
  opponentSan: "d4",
  wholeOpening: true,
});

const holeReport = (): TriageReport =>
  reportOf(
    { gamesSeen: 2 },
    {
      hasCards: true,
      gamesScanned: 199,
      gamesSeen: 199,
      gaps: [HOLE, item({ opponentSan: "Nc3", ply: 3, line: "1. e4 c5 2. Nc3" })],
    },
  );

describe("TriageView — whole-opening holes", () => {
  it("groups the opponent-first-move gap into one labelled row, keeping mid-line gaps as rows", async () => {
    vi.mocked(triageReport).mockResolvedValue(holeReport());
    const { container } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain("No repertoire vs 1. d4 (63 games)"),
    );
    expect(container.textContent).toContain("WHOLE-OPENING HOLES");
    // The mid-line gap keeps its per-position row and the summary splits
    // the two shapes honestly.
    expect(container.textContent).toContain("opponent played Nc3 — no card after it");
    expect(container.textContent).toContain("1 whole-opening hole · 1 in-book gap");
  });

  it("infers from the user's games rooted after the opponent move, then adopts", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(holeReport());
    vi.mocked(triageInferFrom).mockResolvedValue({
      player: "Infer, Ida",
      color: "black",
      gamesScanned: 63,
      lines: [
        {
          sans: ["d4", "Nf6", "c4", "e6"],
          games: 40,
          score: 55,
          eco: "E00",
          openingName: "Indian Defense",
        },
        { sans: ["d4", "d5"], games: 23, score: 47.8, eco: "D00", openingName: "Queen's Pawn" },
      ],
    });
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain("No repertoire vs 1. d4 (63 games)"),
    );
    fireEvent.click(getByText("Infer from your games"));
    await waitFor(() =>
      expect(vi.mocked(triageInferFrom)).toHaveBeenCalledWith("Infer, Ida", "black", ["d4"]),
    );
    await waitFor(() => expect(container.textContent).toContain("Adopt all 2 lines"));
    expect(container.textContent).toContain("40 games · 55% score · E00 Indian Defense");
    fireEvent.click(getByText("Adopt all 2 lines"));
    await waitFor(() => expect(vi.mocked(trainAddLine)).toHaveBeenCalledTimes(2));
    // Plain adds (no conflicting card exists in a hole — no replace).
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith("black", ["d4", "Nf6", "c4", "e6"]);
    expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith("black", ["d4", "d5"]);
    await waitFor(() => expect(vi.mocked(triageReport)).toHaveBeenCalledTimes(2));
  });

  it("board answer on a hole: confirm shows the exact move and target, adopt passes the prefixed line", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(holeReport());
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain("No repertoire vs 1. d4 (63 games)"),
    );
    // Selecting the hole shows its position; the board accepts the
    // user's move (real legality + SAN wiring: g8→f6 is 1...Nf6).
    fireEvent.click(getByText("No repertoire vs 1. d4 (63 games)"));
    expect(container.textContent).toContain("Know your answer? Play it on the board");
    fireEvent.click(getByText("board-g8f6"));
    expect(container.textContent).toContain("Set 1... Nf6 as your repertoire answer to 1. d4?");
    fireEvent.click(getByText("Set as my answer"));
    await waitFor(() =>
      expect(vi.mocked(trainAddLine)).toHaveBeenCalledWith(
        "black",
        ["d4", "Nf6"],
        undefined,
        undefined,
        false,
      ),
    );
    await waitFor(() =>
      expect(container.textContent).toContain('Set Nf6 as your answer in "main (white)"'),
    );
    await waitFor(() => expect(vi.mocked(triageReport)).toHaveBeenCalledTimes(2));
  });

  it("cancel discards the pending answer without writing anything", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(holeReport());
    vi.mocked(trainAddLine).mockClear();
    const { container, getByText } = renderAndRun();
    await waitFor(() =>
      expect(container.textContent).toContain("No repertoire vs 1. d4 (63 games)"),
    );
    fireEvent.click(getByText("No repertoire vs 1. d4 (63 games)"));
    fireEvent.click(getByText("board-g8f6"));
    expect(container.textContent).toContain("Set 1... Nf6 as your repertoire answer to 1. d4?");
    fireEvent.click(getByText("Cancel"));
    expect(container.textContent).not.toContain("Set 1... Nf6");
    expect(container.textContent).toContain("Know your answer? Play it on the board");
    expect(vi.mocked(trainAddLine)).not.toHaveBeenCalled();
  });
});

/* ------------------------------------------------------------------ */
/* Hover-scrub line preview (2026-07-30 field request): hovering a     */
/* prospective line's move tokens walks the aside board through it     */
/* ------------------------------------------------------------------ */

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/** The fen the mock aside board is currently showing. */
const boardFen = (container: HTMLElement): string | null =>
  container.querySelector('[data-testid="board"]')?.getAttribute("data-fen") ?? null;

/** Ground truth: position after `sans` from the standard start. */
const fenAfter = (sans: string[]): string => {
  const r = gameFromSans(sans);
  if (!r.ok) throw new Error("fixture line must replay");
  return r.game.fens[sans.length];
};

describe("TriageView — triage rows scrub too", () => {
  it("hovering a row's moves walks them on the aside board", async () => {
    vi.mocked(triageReport).mockResolvedValue(
      reportOf(
        { gamesSeen: 5, hasCards: true, gaps: [item({ opponentSan: "c5" })] },
        { gamesSeen: 1 },
      ),
    );
    const { container } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("opponent played c5"));
    expect(boardFen(container)).toBe(START_FEN);

    // The row's own line is scrubbable — it is the most obvious thing on
    // the screen to hover, and the aside board is right there.
    const toks = container.querySelectorAll(".triage-row .scrub-tok");
    expect([...toks].map((t) => t.textContent)).toEqual(["1. e4", "c5"]);
    fireEvent.mouseOver(toks[0]);
    expect(boardFen(container)).toBe(fenAfter(["e4"]));
    fireEvent.mouseOut(container.querySelector(".triage-row .scrub-line")!);
    expect(boardFen(container)).toBe(START_FEN);
  });

  it("keeps the row itself the only keyboard stop", async () => {
    vi.mocked(triageReport).mockResolvedValue(
      reportOf(
        { gamesSeen: 5, hasCards: true, gaps: [item({ opponentSan: "c5" })] },
        { gamesSeen: 1 },
      ),
    );
    const { container } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("opponent played c5"));
    // A focusable line nested in the row button would be a keyboard trap.
    expect(container.querySelector(".triage-row .scrub-line")?.getAttribute("tabindex")).toBeNull();
  });
});

describe("TriageView — a running extension shows its work", () => {
  const GAP_FEN = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";

  it("reports depth and the engine's current picks instead of a bare wait", async () => {
    vi.mocked(triageReport).mockResolvedValue(
      reportOf(
        { gamesSeen: 5, hasCards: true, gaps: [item({ fen: GAP_FEN, opponentSan: "c5" })] },
        { gamesSeen: 1 },
      ),
    );
    vi.mocked(triageExtensionStatus).mockResolvedValue({
      extension: null,
      jobStatus: "running",
      jobsAhead: 0,
      workerActive: true,
      search: {
        jobId: 7,
        fen: GAP_FEN,
        depth: 22,
        targetDepth: 30,
        nodes: 41_200_000,
        nps: 1_800_000,
        lines: [{ sans: ["Nf3", "d6", "d4"], scoreCp: 35, mate: null }],
      },
    });
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("opponent played c5"));
    fireEvent.click(getByText(/opponent played c5/));
    await waitFor(() => expect(container.textContent).toContain("EXTEND THE BOOK"));

    await waitFor(() =>
      expect(container.textContent).toContain("depth 22 of 30 · 41.2M nodes · 1.8M/s"),
    );
    // The provisional line is on screen, and marked as provisional.
    // Numbered from the analysed position (after 1.e4 c5), not from move 1.
    expect(container.querySelector(".triage-ext-live")?.textContent).toContain("2. Nf3 d6 3. d4");
    expect(container.textContent).toContain("Nothing is stored until the search finishes");
    // Depth drives the bar, and it is a position — not a percentage of time.
    const bar = container.querySelector('[role="progressbar"]');
    expect(bar?.getAttribute("aria-valuenow")).toBe("22");
    expect(bar?.getAttribute("aria-valuemax")).toBe("30");
  });

  it("says the search is starting when the engine has not reported yet", async () => {
    vi.mocked(triageReport).mockResolvedValue(
      reportOf(
        { gamesSeen: 5, hasCards: true, gaps: [item({ fen: GAP_FEN, opponentSan: "c5" })] },
        { gamesSeen: 1 },
      ),
    );
    vi.mocked(triageExtensionStatus).mockResolvedValue({
      extension: null,
      jobStatus: "running",
      jobsAhead: 0,
      workerActive: true,
      search: null,
    });
    const { container, getByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("opponent played c5"));
    fireEvent.click(getByText(/opponent played c5/));

    await waitFor(() => expect(container.textContent).toContain("Engine starting the search"));
    expect(container.querySelector('[role="progressbar"]')).toBeNull();
  });
});

describe("TriageView — inferred lines read as trunk plus detail", () => {
  it("indents a line that goes deeper into the one above and dims the shared moves", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 5 }, { gamesSeen: 1 }));
    vi.mocked(triageInferRepertoire).mockResolvedValue({
      player: "Infer, Ida",
      color: "white",
      gamesScanned: 20,
      lines: [
        {
          sans: ["e4", "c5", "Nf3"],
          games: 19,
          score: 50,
          eco: "B27",
          openingName: "Sicilian Defense",
        },
        {
          sans: ["e4", "c5", "Nf3", "d6", "d4"],
          games: 4,
          score: 50,
          eco: null,
          openingName: null,
        },
      ],
    });
    const { container } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("1. e4 c5 2. Nf3"));

    const rows = container.querySelectorAll(".triage-infer-line");
    expect(rows).toHaveLength(2);
    expect(rows[0].classList.contains("triage-infer-cont")).toBe(false);
    expect(rows[1].classList.contains("triage-infer-cont")).toBe(true);
    // The trunk's three moves are dimmed in the continuation; what is new
    // (3...d6 4.d4) is not — and every token stays hoverable.
    const dimmed = [...rows[1].querySelectorAll(".scrub-tok.shared")].map((t) => t.textContent);
    expect(dimmed).toEqual(["1. e4", "c5", "2. Nf3"]);
    expect(rows[0].querySelectorAll(".scrub-tok.shared")).toHaveLength(0);
  });
});

describe("TriageView — hover-scrub line preview", () => {
  it("hovering an inferred line's second move drives the aside board; leave restores", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 5 }, { gamesSeen: 1 }));
    vi.mocked(triageInferRepertoire).mockResolvedValue({
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
      ],
    });
    const { container } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("1. e4 c5 2. Nf3"));
    expect(boardFen(container)).toBe(START_FEN);

    const toks = container.querySelectorAll(".triage-infer-line .scrub-tok");
    expect([...toks].map((t) => t.textContent)).toEqual(["1. e4", "c5", "2. Nf3"]);
    fireEvent.mouseOver(toks[1]);
    expect(boardFen(container)).toBe(fenAfter(["e4", "c5"]));
    expect(
      container.querySelector('[data-testid="board"]')?.getAttribute("data-lastmove"),
    ).toBe("c7c5");
    // Honest tiny caption under the board while scrubbing.
    expect(container.querySelector(".scrub-caption")?.textContent).toBe("after 1... c5");

    fireEvent.mouseOut(container.querySelector(".triage-infer-line .scrub-line")!);
    expect(boardFen(container)).toBe(START_FEN);
    expect(container.querySelector(".scrub-caption")).toBeNull();
  });

  it("a live preview suspends the movable set-my-answer board; leave restores both", async () => {
    vi.mocked(triageReport).mockClear();
    vi.mocked(triageReport).mockResolvedValue(realityReport());
    const { container, getByText, queryByText } = renderAndRun();
    await waitFor(() => expect(container.textContent).toContain("Your cards say 1... e5"));
    fireEvent.click(getByText("I know my answer — play it on the board"));
    // Selected item's position, movable for the user's own move.
    expect(boardFen(container)).toBe(AFTER_E4);
    expect(queryByText("board-b8c6")).not.toBeNull();

    // Hover the reality line's third move (2. Nf3): the board scrubs to
    // that position and is NOT movable — the scrub never fights the
    // set-my-answer flow.
    const toks = container.querySelectorAll(".triage-reality .scrub-tok");
    fireEvent.mouseOver(toks[2]);
    expect(boardFen(container)).toBe(fenAfter(["e4", "c5", "Nf3"]));
    expect(queryByText("board-b8c6")).toBeNull();
    expect(container.querySelector(".scrub-caption")?.textContent).toBe("after 2. Nf3");

    // Leaving restores the item position AND the movable behavior.
    fireEvent.mouseOut(container.querySelector(".triage-reality .scrub-line")!);
    expect(boardFen(container)).toBe(AFTER_E4);
    expect(queryByText("board-b8c6")).not.toBeNull();
  });
});

describe("TriageView — identity is canonical (2026-07-30 ruling)", () => {
  it("renders no name input; auto-runs for the self identity with a Profile pointer", async () => {
    vi.mocked(triageReport).mockResolvedValue(reportOf({ gamesSeen: 5 }, { gamesSeen: 1 }));
    const { container } = render(<TriageView onOpenGameAt={vi.fn()} />);
    expect(container.querySelector("input")).toBeNull();
    await waitFor(() =>
      expect(vi.mocked(triageReport)).toHaveBeenCalledWith("Infer, Ida"),
    );
    await waitFor(() => expect(container.textContent).toContain("for Infer, Ida"));
    expect(container.textContent).toContain("change on Profile");
  });

  it("without a self identity: honest setup state pointing at Profile", async () => {
    const { selfPlayerGet } = await import("./lib/db");
    vi.mocked(selfPlayerGet).mockResolvedValueOnce(null);
    vi.mocked(triageReport).mockClear();
    const onNavigate = vi.fn();
    const { container, getByText } = render(
      <TriageView onOpenGameAt={vi.fn()} onNavigate={onNavigate} />,
    );
    await waitFor(() =>
      expect(container.textContent).toContain("doesn't know who you are yet"),
    );
    fireEvent.click(getByText("Set up on Profile"));
    expect(onNavigate).toHaveBeenCalledWith("profile");
    expect(vi.mocked(triageReport)).not.toHaveBeenCalled();
  });
});
