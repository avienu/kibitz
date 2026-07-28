import { describe, expect, it } from "vitest";
import {
  clocksAt,
  estimatedSpeed,
  FAIR_PLAY_NOTICE,
  fmtClock,
  isTerminal,
  legalDests,
  numberedSans,
  resultLine,
  stepsFromUci,
  turnOf,
  type GameSnapshot,
} from "./lichessPlay";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const snap = (over: Partial<GameSnapshot>): GameSnapshot => ({
  gameId: "abc123",
  myColor: "white",
  white: "SomeUser",
  black: "Opponent",
  whiteRating: 1500,
  blackRating: 1520,
  speed: "rapid",
  rated: false,
  initialFen: START_FEN,
  moves: [],
  status: "started",
  winner: null,
  wtimeMs: 600_000,
  btimeMs: 600_000,
  wincMs: 5_000,
  bincMs: 5_000,
  wdraw: false,
  bdraw: false,
  ...over,
});

describe("estimatedSpeed (Board API time-control policy mirror)", () => {
  it("rejects bullet and blitz — lichess forbids them for third-party clients", () => {
    expect(estimatedSpeed(1, 0)).toBeNull();
    expect(estimatedSpeed(3, 2)).toBeNull();
    expect(estimatedSpeed(5, 3)).toBeNull(); // 7 min estimate
  });
  it("classifies rapid and classical by the lichess estimate", () => {
    expect(estimatedSpeed(8, 0)).toBe("rapid");
    expect(estimatedSpeed(10, 5)).toBe("rapid");
    expect(estimatedSpeed(25, 0)).toBe("classical");
    expect(estimatedSpeed(15, 30)).toBe("classical");
  });
});

describe("stepsFromUci", () => {
  it("replays UCI moves into fens + sans", () => {
    const s = stepsFromUci(START_FEN, ["e2e4", "e7e5", "g1f3"]);
    expect(s).not.toBeNull();
    expect(s!.sans).toEqual(["e4", "e5", "Nf3"]);
    expect(s!.fens).toHaveLength(4);
    expect(s!.fens[0]).toBe(START_FEN);
    expect(s!.fens[1]).toContain(" b "); // black to move after 1. e4
  });
  it("handles promotion suffixes", () => {
    const fen = "8/4P1k1/8/8/8/8/8/4K3 w - - 0 1";
    const s = stepsFromUci(fen, ["e7e8q"]);
    expect(s!.sans).toEqual(["e8=Q"]);
  });
  it("returns null on an illegal move instead of guessing", () => {
    expect(stepsFromUci(START_FEN, ["e2e5"])).toBeNull();
    expect(stepsFromUci("not a fen", ["e2e4"])).toBeNull();
  });
});

describe("turn / terminal state", () => {
  it("derives the side to move from the move count", () => {
    expect(turnOf({ moves: [] })).toBe("white");
    expect(turnOf({ moves: ["e2e4"] })).toBe("black");
    expect(turnOf({ moves: ["e2e4", "e7e5"] })).toBe("white");
  });
  it("treats everything but created/started as terminal", () => {
    for (const s of ["mate", "resign", "draw", "outoftime", "aborted", "stalemate"]) {
      expect(isTerminal(s)).toBe(true);
    }
    for (const s of ["", "created", "started"]) {
      expect(isTerminal(s)).toBe(false);
    }
  });
});

describe("legalDests", () => {
  it("maps origins to legal destinations", () => {
    const d = legalDests(START_FEN);
    expect(d).not.toBeNull();
    expect(d!.get("e2")).toEqual(expect.arrayContaining(["e3", "e4"]));
    expect(legalDests("garbage")).toBeNull();
  });
});

describe("clock display", () => {
  it("formats m:ss under an hour and h:mm:ss above, never negative", () => {
    expect(fmtClock(65_000)).toBe("1:05");
    expect(fmtClock(600_000)).toBe("10:00");
    expect(fmtClock(3_723_000)).toBe("1:02:03");
    expect(fmtClock(-5_000)).toBe("0:00");
  });

  it("ticks only the side to move, only once both sides have moved", () => {
    // Before move 2 the clocks do not run (matching lichess).
    const early = clocksAt(snap({ moves: ["e2e4"] }), 1_000, 6_000);
    expect(early).toEqual({ whiteMs: 600_000, blackMs: 600_000 });
    // After both moved, white (to move) ticks down; black holds.
    const live = clocksAt(snap({ moves: ["e2e4", "e7e5"] }), 1_000, 6_000);
    expect(live.whiteMs).toBe(595_000);
    expect(live.blackMs).toBe(600_000);
    // A finished game freezes both clocks.
    const done = clocksAt(snap({ moves: ["e2e4", "e7e5"], status: "mate" }), 1_000, 60_000);
    expect(done).toEqual({ whiteMs: 600_000, blackMs: 600_000 });
  });
});

describe("numberedSans", () => {
  it("renders the standard numbered move strip", () => {
    expect(numberedSans(["e4", "e5", "Nf3"])).toBe("1. e4 e5 2. Nf3");
    expect(numberedSans([])).toBe("");
  });
});

describe("resultLine", () => {
  it("is null while the game runs", () => {
    expect(resultLine(snap({}))).toBeNull();
  });
  it("names the ending and the winner, from the player's side", () => {
    expect(resultLine(snap({ status: "mate", winner: "white" }))).toBe(
      "Checkmate · SomeUser wins — you won",
    );
    expect(resultLine(snap({ status: "resign", winner: "black" }))).toBe(
      "Resignation · Opponent wins — you lost",
    );
    expect(resultLine(snap({ status: "draw" }))).toBe("Draw");
    expect(resultLine(snap({ status: "aborted" }))).toBe("Aborted");
  });
});

describe("fair-play notice", () => {
  it("states that assistance is disabled during play, citing lichess ToS", () => {
    expect(FAIR_PLAY_NOTICE).toMatch(/Engine assistance is disabled/);
    expect(FAIR_PLAY_NOTICE).toMatch(/lichess Terms of Service/);
  });
});
