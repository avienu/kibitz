// @vitest-environment jsdom
/**
 * Opponent-prep build-out tests (round-2 spec): stepper value persistence
 * and back-navigation, opponent-param prefill, prep_state recorded once.
 */
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PrepView from "./PrepView";
import * as db from "./lib/db";

// The aside board is chessground-backed — not what these tests exercise.
vi.mock("./Board", () => ({
  default: () => <div data-testid="board" />,
}));

vi.mock("./lib/db", () => ({
  matchingPlayers: vi.fn(),
  listGames: vi.fn(),
  prepFingerprint: vi.fn(),
  prepView: vi.fn(),
  prepStateGet: vi.fn(),
  prepStateSet: vi.fn(),
  getGame: vi.fn(),
}));

const mocked = vi.mocked(db);

const FINGERPRINT: db.PrepFingerprint = {
  games: 148,
  scorePct: 47.5,
  rows: [{ eco: "B22", name: "Sicilian, Alapin", games: 21, sharePct: 14, scorePct: 38 }],
  bookExits: [
    { hash: "9", eco: "B22", openingName: "Sicilian, Alapin", san: "Nc6", ply: 5, count: 17, scorePct: 38 },
  ],
};

const LINES: db.WeakLine[] = [
  {
    hash: "123",
    eco: "B22",
    openingName: "Sicilian, Alapin",
    ply: 4,
    opponentMoves: ["Nc6"],
    games: 21,
    scorePct: 38,
    weakness: 2.5,
    deviation: false,
    masterGames: [
      {
        gameId: 55,
        white: "Sveshnikov, E.",
        black: "Kramnik, V.",
        whiteElo: 2600,
        blackElo: 2750,
        event: "Russian Ch",
        date: "1998.05.01",
        result: "1/2-1/2",
        ply: 4,
      },
    ],
  },
];

beforeEach(() => {
  vi.clearAllMocks();
  mocked.matchingPlayers.mockResolvedValue(["R. Halvorsen"]);
  mocked.listGames.mockResolvedValue({ total: 148, rows: [] });
  mocked.prepFingerprint.mockResolvedValue(FINGERPRINT);
  mocked.prepView.mockResolvedValue(LINES);
  mocked.prepStateGet.mockResolvedValue([]);
  mocked.prepStateSet.mockResolvedValue(undefined);
  mocked.getGame.mockResolvedValue({
    id: 55,
    white: "Sveshnikov, E.",
    black: "Kramnik, V.",
    whiteElo: 2600,
    blackElo: 2750,
    event: "Russian Ch",
    site: "?",
    round: null,
    date: "1998.05.01",
    result: "1/2-1/2",
    eco: "B22",
    openingName: "Sicilian, Alapin",
    plyCount: 4,
    startFen: null,
    sans: ["e4", "c5", "c3", "Nf6"],
  });
});

afterEach(cleanup);

function renderPrep(opponent: string | null = "Halvorsen") {
  const onLoadGameAt = vi.fn();
  const onNavigate = vi.fn();
  const utils = render(
    <PrepView
      onLoadGameAt={onLoadGameAt}
      profile={null}
      opponent={opponent}
      onNavigate={onNavigate}
    />,
  );
  return { ...utils, onLoadGameAt, onNavigate };
}

describe("Prep — opponent param prefill (Home's Go)", () => {
  it("prefills step 1 and searches immediately", async () => {
    const { findByText, getByPlaceholderText } = renderPrep("Halvorsen");
    expect((getByPlaceholderText("Opponent name…") as HTMLInputElement).value).toBe("Halvorsen");
    expect(mocked.matchingPlayers).toHaveBeenCalledWith("Halvorsen");
    expect(await findByText("R. Halvorsen")).toBeTruthy();
    expect(await findByText("148 games")).toBeTruthy();
  });

  it("does not search on its own without the param", () => {
    renderPrep(null);
    expect(mocked.matchingPlayers).not.toHaveBeenCalled();
  });
});

describe("Prep — stepper persistence and back-navigation", () => {
  it("selection advances to step 2, chips keep their values, back-nav is free", async () => {
    const { findByText, getByText, getAllByText, getByPlaceholderText } = renderPrep();
    fireEvent.click(await findByText("R. Halvorsen"));

    // Step 2: fingerprint fetched for the selection, default colour black.
    await waitFor(() =>
      expect(mocked.prepFingerprint).toHaveBeenCalledWith("R. Halvorsen", "black"),
    );
    expect(await findByText("Sicilian, Alapin")).toBeTruthy();
    // Chip ① shows the chosen opponent; chip ② shows the colour (the
    // segmented control also says "as Black", hence getAllByText).
    expect(getAllByText("R. Halvorsen").length).toBeGreaterThan(0);
    expect(getAllByText("as Black").length).toBeGreaterThan(0);

    // Free backward navigation: chip ① → step 1, selections intact.
    fireEvent.click(getByText("Opponent"));
    expect((getByPlaceholderText("Opponent name…") as HTMLInputElement).value).toBe("Halvorsen");
    // Forward onto a reached step works too.
    fireEvent.click(getByText("Fingerprint"));
    expect(getByText("REPERTOIRE FINGERPRINT")).toBeTruthy();
  });

  it("weak-line selection carries its name and master count into the chips", async () => {
    const { findByText, getByText } = renderPrep();
    fireEvent.click(await findByText("R. Halvorsen"));
    // Jump to step 3 by clicking the fingerprint row.
    fireEvent.click(await findByText("Sicilian, Alapin"));
    // Top-ranked card → step 4.
    fireEvent.click(await findByText("38% in 21"));
    expect(getByText("MASTER GAMES IN THIS EXACT POSITION")).toBeTruthy();
    expect(getByText("1 game")).toBeTruthy(); // chip ④ value
    expect(getByText("Sveshnikov, E.")).toBeTruthy();
  });
});

describe("Prep — prep_state recorded once at step-2 entry", () => {
  it("records {opponent, color} exactly once despite re-entering step 2", async () => {
    const { findByText, getByText } = renderPrep();
    fireEvent.click(await findByText("R. Halvorsen"));
    await waitFor(() => expect(mocked.prepStateSet).toHaveBeenCalledTimes(1));
    const entries = mocked.prepStateSet.mock.calls[0][0];
    expect(entries[0].opponent).toBe("R. Halvorsen");
    expect(entries[0].color).toBe("black");

    // Back to step 1 and into step 2 again — no second record.
    fireEvent.click(getByText("Opponent"));
    fireEvent.click(getByText("Fingerprint"));
    await waitFor(() => expect(getByText("REPERTOIRE FINGERPRINT")).toBeTruthy());
    expect(mocked.prepStateSet).toHaveBeenCalledTimes(1);
  });
});

describe("Prep — honest surfaces", () => {
  it("aside finding is honestly absent without the opponent's profile", async () => {
    const { findByText, getByText } = renderPrep();
    fireEvent.click(await findByText("R. Halvorsen"));
    await waitFor(() =>
      expect(getByText(/No profile has been built for R\. Halvorsen/)).toBeTruthy(),
    );
  });

  it("account-fetch buttons are disabled (sync is CLI-only today)", () => {
    const { getByText } = renderPrep(null);
    expect((getByText("Fetch from Lichess") as HTMLButtonElement).disabled).toBe(true);
    expect((getByText("Fetch from chess.com") as HTMLButtonElement).disabled).toBe(true);
  });
});
