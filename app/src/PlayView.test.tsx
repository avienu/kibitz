// @vitest-environment jsdom
/**
 * Play screen tests (run 10). Two layers:
 *
 * 1. FAIR-PLAY STRUCTURAL GATE — while a lichess game is in progress no
 *    engine, coach explain, live analysis or suggestion surface may be
 *    reachable from the play screen. That is enforced structurally (the
 *    view simply has no such affordances); the gate below fails the build
 *    if anyone ever imports one of those modules into PlayView or its
 *    lib, or drops the visible assistance-disabled notice.
 * 2. Behaviour: token-missing state, rejoin flow, live-game surface.
 */
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import playViewSrc from "./PlayView.tsx?raw";
import lichessPlaySrc from "./lib/lichessPlay.ts?raw";
import PlayView from "./PlayView";
import {
  FAIR_PLAY_NOTICE,
  lichessTokenStatus,
  nowPlaying,
  playJoin,
  playResign,
  type GameSnapshot,
} from "./lib/lichessPlay";

vi.mock("./Board", () => ({
  default: () => <div data-testid="board" />,
}));

vi.mock("./lib/lichessPlay", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/lichessPlay")>();
  return {
    ...actual,
    lichessTokenStatus: vi.fn(),
    playStart: vi.fn(() => Promise.resolve(true)),
    nowPlaying: vi.fn(() => Promise.resolve([])),
    playJoin: vi.fn(() => Promise.resolve(null)),
    playMove: vi.fn(() => Promise.resolve()),
    playResign: vi.fn(() => Promise.resolve()),
    playAbort: vi.fn(() => Promise.resolve()),
    playDraw: vi.fn(() => Promise.resolve()),
    playSeek: vi.fn(() => Promise.resolve()),
    seekCancel: vi.fn(() => Promise.resolve(true)),
    onPlayEvent: vi.fn(() => Promise.resolve(() => {})),
    onPlayGame: vi.fn(() => Promise.resolve(() => {})),
    onPlaySeek: vi.fn(() => Promise.resolve(() => {})),
  };
});

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const liveSnap: GameSnapshot = {
  gameId: "abc123",
  myColor: "white",
  white: "SomeUser",
  black: "Opponent",
  whiteRating: 1500,
  blackRating: 1520,
  speed: "rapid",
  rated: false,
  initialFen: START_FEN,
  moves: ["e2e4", "e7e5"],
  status: "started",
  winner: null,
  wtimeMs: 600_000,
  btimeMs: 598_000,
  wincMs: 5_000,
  bincMs: 5_000,
  wdraw: false,
  bdraw: false,
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/* ------------------------------------------------------------------ */
/* 1. Fair-play structural gate                                        */
/* ------------------------------------------------------------------ */

describe("fair play is structural (lichess ToS)", () => {
  const playSources = `${playViewSrc}\n${lichessPlaySrc}`;

  it("the play surface imports no engine/explain/analysis module", () => {
    // Every assistance surface in the app lives behind these modules;
    // none may ever be reachable from the play screen.
    for (const forbidden of [
      "lib/engine",
      "lib/liveAnalysis",
      "lib/analyses",
      "lib/engineView",
      "lib/evidence",
      "ExplainPanel",
      "EvalBar",
      "explainPosition",
      "analyze_position",
    ]) {
      expect(playSources, `play surface must not reference ${forbidden}`).not.toContain(
        forbidden,
      );
    }
  });

  it("the assistance-disabled notice is rendered by the view", () => {
    expect(playSources).toContain("FAIR_PLAY_NOTICE");
    expect(FAIR_PLAY_NOTICE).toContain("Engine assistance is disabled");
  });
});

/* ------------------------------------------------------------------ */
/* 2. Behaviour                                                        */
/* ------------------------------------------------------------------ */

function renderPlay(onNavigate = vi.fn()) {
  return { onNavigate, ...render(<PlayView treatment="walnut" onNavigate={onNavigate} />) };
}

describe("PlayView — no token", () => {
  beforeEach(() => {
    vi.mocked(lichessTokenStatus).mockResolvedValue({
      configured: false,
      username: null,
      tokenTail: null,
    });
  });

  it("explains the board:play token and routes to Settings", async () => {
    const { getByText, onNavigate } = renderPlay();
    await waitFor(() => expect(getByText(/board:play/)).toBeTruthy());
    fireEvent.click(getByText("Open Settings"));
    expect(onNavigate).toHaveBeenCalledWith("settings");
  });

  it("still shows the fair-play stance", async () => {
    const { getByText } = renderPlay();
    await waitFor(() => expect(getByText(/Engine assistance is disabled/)).toBeTruthy());
  });
});

describe("PlayView — with a token", () => {
  beforeEach(() => {
    vi.mocked(lichessTokenStatus).mockResolvedValue({
      configured: true,
      username: "SomeUser",
      tokenTail: "XYZW",
    });
  });

  it("offers rapid/classical/correspondence only and says why", async () => {
    const { getByText } = renderPlay();
    await waitFor(() => expect(getByText(/no bullet or blitz/)).toBeTruthy());
    expect(getByText("10+0")).toBeTruthy();
    expect(getByText("Correspondence")).toBeTruthy();
  });

  it("lists ongoing games and rejoins onto the board", async () => {
    vi.mocked(nowPlaying).mockResolvedValue([
      {
        gameId: "abc123",
        color: "white",
        opponent: "Opponent",
        isMyTurn: true,
        speed: "rapid",
        lastMove: "e7e5",
        secondsLeft: 600,
      },
    ]);
    vi.mocked(playJoin).mockResolvedValue(liveSnap);
    const { getByText, getByTestId } = renderPlay();
    await waitFor(() => expect(getByText(/vs Opponent · rapid · your move/)).toBeTruthy());
    fireEvent.click(getByText("Rejoin"));
    await waitFor(() => expect(getByTestId("board")).toBeTruthy());
    expect(playJoin).toHaveBeenCalledWith("abc123");
    // Live-game chrome: clocks, players, resign — and no analysis affordance.
    expect(getByText(/SomeUser \(1500\) — you/)).toBeTruthy();
    expect(getByText("Resign")).toBeTruthy();
    expect(getByText("Offer draw")).toBeTruthy();
    expect(getByText(/Engine assistance is disabled/)).toBeTruthy();
  });

  it("resign asks for confirmation before posting", async () => {
    vi.mocked(nowPlaying).mockResolvedValue([
      {
        gameId: "abc123",
        color: "white",
        opponent: "Opponent",
        isMyTurn: true,
        speed: "rapid",
        lastMove: "e7e5",
        secondsLeft: 600,
      },
    ]);
    vi.mocked(playJoin).mockResolvedValue(liveSnap);
    const { getByText } = renderPlay();
    await waitFor(() => expect(getByText("Rejoin")).toBeTruthy());
    fireEvent.click(getByText("Rejoin"));
    await waitFor(() => expect(getByText("Resign")).toBeTruthy());
    fireEvent.click(getByText("Resign"));
    expect(playResign).not.toHaveBeenCalled();
    fireEvent.click(getByText("Confirm resign"));
    expect(playResign).toHaveBeenCalledWith("abc123");
  });

  it("a finished game shows the result and the import pointer", async () => {
    vi.mocked(nowPlaying).mockResolvedValue([
      {
        gameId: "abc123",
        color: "white",
        opponent: "Opponent",
        isMyTurn: false,
        speed: "rapid",
        lastMove: "e7e5",
        secondsLeft: 600,
      },
    ]);
    vi.mocked(playJoin).mockResolvedValue({
      ...liveSnap,
      status: "mate",
      winner: "white",
    });
    const { getByText, queryByText } = renderPlay();
    await waitFor(() => expect(getByText("Rejoin")).toBeTruthy());
    fireEvent.click(getByText("Rejoin"));
    await waitFor(() => expect(getByText(/Checkmate · SomeUser wins — you won/)).toBeTruthy());
    expect(getByText(/imported for review/)).toBeTruthy();
    expect(queryByText("Resign")).toBeNull();
  });
});
