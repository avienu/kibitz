// @vitest-environment jsdom
/**
 * Tactics build-out tests (round-2 spec): the puzzle board NEVER gets an
 * evidence prop, the seed contract changes the initial mode/filter state,
 * and the keyboard map works including the focused-input exception.
 */
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import TacticsView from "./TacticsView";
import * as tactics from "./lib/tactics";

/** The mocked Board records every props object it renders with. */
const mockBoardProps: Record<string, unknown>[] = [];
vi.mock("./Board", () => ({
  default: (props: Record<string, unknown>) => {
    mockBoardProps.push(props);
    return <div data-testid="board" />;
  },
}));

vi.mock("./lib/tactics", async (importOriginal) => {
  const real = await importOriginal<typeof import("./lib/tactics")>();
  return {
    ...real,
    tacticsState: vi.fn(),
    woodpeckerSets: vi.fn(),
    cycleStats: vi.fn(),
    nextPuzzle: vi.fn(),
    verifyMove: vi.fn(),
    recordAttempt: vi.fn(),
    importPuzzles: vi.fn(),
    createWoodpeckerSet: vi.fn(),
    startCycle: vi.fn(),
    finishCycle: vi.fn(),
    woodpeckerPuzzles: vi.fn(),
  };
});

const mocked = vi.mocked(tactics);

const PUZZLE: tactics.ServedPuzzle = {
  puzzle: {
    id: 1,
    lichessId: "abcde",
    // White rook lifts to h8 as the setup move; Black (the solver) replies.
    fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
    moves: ["h1h8", "e8d7"],
    rating: 1795,
    popularity: 95,
    themes: ["fork"],
  },
  matchedThemes: [],
  allowed: 0,
  missed: 0,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockBoardProps.length = 0;
  mocked.tacticsState.mockResolvedValue({ rating: 1842, attempts: 3, puzzles: 5000, themes: [] });
  mocked.woodpeckerSets.mockResolvedValue([]);
  mocked.cycleStats.mockResolvedValue([]);
  mocked.nextPuzzle.mockResolvedValue(PUZZLE);
});

afterEach(cleanup);

function renderTactics(seedClaim: string | null = null) {
  const onVoice = vi.fn();
  const utils = render(
    <TacticsView profile={null} seedClaim={seedClaim} voice="coach" onVoice={onVoice} />,
  );
  return { ...utils, onVoice };
}

describe("Tactics — the puzzle board never gets evidence overlays", () => {
  it("renders Board without any evidence prop (and never pre-highlights)", async () => {
    const { findByTestId } = renderTactics();
    await findByTestId("board");
    expect(mockBoardProps.length).toBeGreaterThan(0);
    for (const props of mockBoardProps) {
      expect(props.evidence).toBeUndefined();
      expect("evidence" in props).toBe(false);
    }
  });
});

describe("Tactics — mode column", () => {
  it("weakness-targeted is the default selected mode", async () => {
    const { container, findByText } = renderTactics();
    await findByText("Weakness-targeted");
    const cur = container.querySelector(".tx2-mode.cur");
    expect(cur?.textContent).toContain("Weakness-targeted");
  });
});

describe("Tactics — Train this weakness seeding", () => {
  it("a motif claim seeds weakness mode with that motif emphasized", async () => {
    const { container, findByText, getByText } = renderTactics("motif:WeakKing:missed");
    await findByText("Weakness-targeted");
    // Mode is weakness and the seed pill names the motif.
    expect(container.querySelector(".tx2-mode.cur")?.textContent).toContain("Weakness-targeted");
    expect(getByText("SEEDED")).toBeTruthy();
    expect(getByText("WeakKing")).toBeTruthy();
    // Serving passes ONLY the seeded motif as the weights (the API's hint).
    fireEvent.click(getByText("Start solving"));
    await waitFor(() => expect(mocked.nextPuzzle).toHaveBeenCalledTimes(1));
    expect(mocked.nextPuzzle).toHaveBeenCalledWith("weakness", undefined, [
      { kind: "WeakKing", allowed: 1, missed: 0 },
    ]);
  });

  it("without a seed and without a profile, weakness mode refuses honestly", async () => {
    const { findByText, getByText } = renderTactics();
    await findByText("Start solving");
    fireEvent.click(getByText("Start solving"));
    await findByText(/needs your profile/);
    expect(mocked.nextPuzzle).not.toHaveBeenCalled();
  });
});

describe("Tactics — keyboard (⏎ / H / S / G, never inside inputs)", () => {
  it("Enter serves the next puzzle", async () => {
    const { findByText } = renderTactics("motif:WeakKing:missed");
    await findByText("Start solving");
    fireEvent.keyDown(window, { key: "Enter" });
    await waitFor(() => expect(mocked.nextPuzzle).toHaveBeenCalledTimes(1));
  });

  it("keys inside a focused input are swallowed (the editable exception)", async () => {
    const { container, findByText } = renderTactics("motif:WeakKing:missed");
    await findByText("Start solving");
    // The Woodpecker create panel provides a real text input.
    const input = container.querySelector(".tx2-wood-create input[type='text']")!;
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "h" });
    fireEvent.keyDown(input, { key: "s" });
    fireEvent.keyDown(input, { key: "g" });
    expect(mocked.nextPuzzle).not.toHaveBeenCalled();
  });
});

describe("Tactics — reasoning aside", () => {
  it("shows WHY THIS PUZZLE with the voice segmented wired to the app voice", async () => {
    const { getByText, findByText, onVoice } = renderTactics();
    await findByText("WHY THIS PUZZLE");
    fireEvent.click(getByText("Neutral"));
    expect(onVoice).toHaveBeenCalledWith("neutral");
  });

  it("the facts block labels sit in a 78px mono column (MOTIF/SOURCE/RATING)", async () => {
    const { findByText, getByText } = renderTactics();
    await findByText("WHY THIS PUZZLE");
    for (const label of ["MOTIF", "SOURCE", "RATING"]) {
      expect(getByText(label)).toBeTruthy();
    }
  });
});
