/**
 * Lichess Board API play surface (run 10): typed wrappers over the
 * src-tauri/src/lichess_play.rs IPC commands and events, plus the pure
 * view-model helpers the Play screen uses (unit-tested offline in
 * lichessPlay.test.ts).
 *
 * FAIR PLAY (lichess ToS — a product principle like engine-off): this
 * module and the Play screen carry NO engine, explain, or analysis
 * affordance of any kind. The notice below is rendered visibly while
 * playing; enforcement is structural — PlayView simply mounts none of
 * those surfaces (see PlayView.test.tsx for the regression gate).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Chess } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { makeFen, parseFen } from "chessops/fen";
import { makeSanAndPlay } from "chessops/san";
import { parseUci } from "chessops/util";

/** Shown on the Play screen whenever a game is in progress. */
export const FAIR_PLAY_NOTICE =
  "Engine assistance is disabled while you play: no analysis, no explanations, " +
  "no suggestions (lichess Terms of Service). The finished game imports " +
  "automatically for review afterwards.";

/* ---- IPC types (serde camelCase mirrors of lichess_play.rs) ---- */

export interface LichessTokenStatus {
  configured: boolean;
  username: string | null;
  /** Last 4 characters only — the token itself never reaches the UI. */
  tokenTail: string | null;
}

export interface GameSnapshot {
  gameId: string;
  /** "white" | "black"; null when the account is not a player. */
  myColor: string | null;
  white: string;
  black: string;
  whiteRating: number | null;
  blackRating: number | null;
  /** "rapid" | "classical" | "correspondence" (as lichess reports it). */
  speed: string;
  rated: boolean;
  initialFen: string;
  /** UCI moves from the initial position. */
  moves: string[];
  /** "created" | "started" | terminal statuses (mate, resign, …). */
  status: string;
  winner: string | null;
  wtimeMs: number;
  btimeMs: number;
  wincMs: number;
  bincMs: number;
  wdraw: boolean;
  bdraw: boolean;
}

export interface PlayEvent {
  /** "gameStart" | "gameFinish" | "imported" | "error". */
  kind: string;
  gameId: string;
  /** Opponent username (game events) or the error message. */
  detail: string | null;
}

export interface SeekEvent {
  active: boolean;
  error: string | null;
}

export interface NowPlaying {
  gameId: string;
  color: string;
  opponent: string;
  isMyTurn: boolean;
  speed: string;
  lastMove: string;
  secondsLeft: number | null;
}

/* ---- token (secret stays Rust-side; only status crosses IPC) ---- */

export function lichessTokenStatus(): Promise<LichessTokenStatus> {
  return invoke<LichessTokenStatus>("lichess_token_status");
}

export function lichessTokenSet(token: string): Promise<LichessTokenStatus> {
  return invoke<LichessTokenStatus>("lichess_token_set", { token });
}

export function lichessTokenClear(): Promise<LichessTokenStatus> {
  return invoke<LichessTokenStatus>("lichess_token_clear");
}

/* ---- play commands ---- */

/** Start the account event stream (idempotent; false = already running). */
export function playStart(): Promise<boolean> {
  return invoke<boolean>("lichess_play_start");
}

/** Ensure a board stream for the game; returns the last known snapshot. */
export function playJoin(gameId: string): Promise<GameSnapshot | null> {
  return invoke<GameSnapshot | null>("lichess_play_join", { gameId });
}

export function playMove(gameId: string, uci: string): Promise<void> {
  return invoke<void>("lichess_play_move", { gameId, uci });
}

export function playResign(gameId: string): Promise<void> {
  return invoke<void>("lichess_play_resign", { gameId });
}

export function playAbort(gameId: string): Promise<void> {
  return invoke<void>("lichess_play_abort", { gameId });
}

/** Offer/accept a draw (true) or decline the pending offer (false). */
export function playDraw(gameId: string, accept: boolean): Promise<void> {
  return invoke<void>("lichess_play_draw", { gameId, accept });
}

/** Realtime seek (rapid/classical) or, with `days`, correspondence. */
export function playSeek(opts: {
  minutes?: number;
  increment?: number;
  days?: number;
  rated: boolean;
  color?: string;
}): Promise<void> {
  return invoke<void>("lichess_play_seek", {
    minutes: opts.minutes ?? null,
    increment: opts.increment ?? null,
    days: opts.days ?? null,
    rated: opts.rated,
    color: opts.color ?? null,
  });
}

export function seekCancel(): Promise<boolean> {
  return invoke<boolean>("lichess_seek_cancel");
}

export function nowPlaying(): Promise<NowPlaying[]> {
  return invoke<NowPlaying[]>("lichess_now_playing");
}

/* ---- events ---- */

export function onPlayEvent(cb: (ev: PlayEvent) => void): Promise<UnlistenFn> {
  return listen<PlayEvent>("lichess-play-event", (e) => cb(e.payload));
}

export function onPlayGame(cb: (snap: GameSnapshot) => void): Promise<UnlistenFn> {
  return listen<GameSnapshot>("lichess-play-game", (e) => cb(e.payload));
}

export function onPlaySeek(cb: (ev: SeekEvent) => void): Promise<UnlistenFn> {
  return listen<SeekEvent>("lichess-play-seek", (e) => cb(e.payload));
}

/* ---- pure helpers (unit-tested in lichessPlay.test.ts) ---- */

/**
 * Speed class of a realtime seek per lichess's estimate
 * (minutes*60 + 40*increment seconds); null when the Board API forbids it
 * for third-party clients (bullet/blitz — under 8 minutes estimated).
 * Mirrors the Rust-side policy in lichess_play.rs, which is authoritative.
 */
export function estimatedSpeed(minutes: number, increment: number): "rapid" | "classical" | null {
  const est = minutes * 60 + 40 * increment;
  if (est < 480) return null;
  return est < 1500 ? "rapid" : "classical";
}

export function turnOf(snap: Pick<GameSnapshot, "moves">): "white" | "black" {
  return snap.moves.length % 2 === 0 ? "white" : "black";
}

/** A status other than created/started ends the game. */
export function isTerminal(status: string): boolean {
  return status !== "" && status !== "created" && status !== "started";
}

export interface PlaySteps {
  /** Position after each ply; fens[0] is the initial position. */
  fens: string[];
  sans: string[];
}

/**
 * Replay the UCI move list from the initial FEN. Null on any illegal or
 * unparseable move (the board then shows the initial position rather
 * than something wrong).
 */
export function stepsFromUci(initialFen: string, ucis: readonly string[]): PlaySteps | null {
  const setup = parseFen(initialFen);
  if (setup.isErr) return null;
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return null;
  const pos = p.unwrap();
  const fens = [makeFen(pos.toSetup())];
  const sans: string[] = [];
  for (const uci of ucis) {
    const move = parseUci(uci);
    if (!move || !pos.isLegal(move)) return null;
    sans.push(makeSanAndPlay(pos, move));
    fens.push(makeFen(pos.toSetup()));
  }
  return { fens, sans };
}

/** Legal chessground destination map for `fen`; null on a bad FEN. */
export function legalDests(fen: string): Map<string, string[]> | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return null;
  return chessgroundDests(p.unwrap());
}

/** "1. e4 e5 2. Nf3 …" from a SAN list (for the move strip). */
export function numberedSans(sans: readonly string[]): string {
  const parts: string[] = [];
  for (let i = 0; i < sans.length; i++) {
    if (i % 2 === 0) parts.push(`${i / 2 + 1}.`);
    parts.push(sans[i]);
  }
  return parts.join(" ");
}

/** "m:ss" under an hour, "h:mm:ss" above. Never negative. */
export function fmtClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

/**
 * Clock values at `nowMs`, ticking the side to move down locally from
 * the last server snapshot (received at `receivedAtMs`). Clocks only run
 * once both sides have moved and the game is live — matching lichess.
 */
export function clocksAt(
  snap: Pick<GameSnapshot, "moves" | "status" | "wtimeMs" | "btimeMs">,
  receivedAtMs: number,
  nowMs: number,
): { whiteMs: number; blackMs: number } {
  const elapsed = Math.max(0, nowMs - receivedAtMs);
  const ticking = snap.status === "started" && snap.moves.length >= 2;
  const turn = turnOf(snap);
  return {
    whiteMs: Math.max(0, snap.wtimeMs - (ticking && turn === "white" ? elapsed : 0)),
    blackMs: Math.max(0, snap.btimeMs - (ticking && turn === "black" ? elapsed : 0)),
  };
}

const STATUS_NAMES: Record<string, string> = {
  mate: "Checkmate",
  resign: "Resignation",
  outoftime: "Flag fell",
  timeout: "Left the game",
  draw: "Draw",
  stalemate: "Stalemate",
  aborted: "Aborted",
  nostart: "Never started",
};

/** One-line result for a finished game; null while it is in progress. */
export function resultLine(
  snap: Pick<GameSnapshot, "status" | "winner" | "white" | "black" | "myColor">,
): string | null {
  if (!isTerminal(snap.status)) return null;
  const what = STATUS_NAMES[snap.status] ?? snap.status;
  if (!snap.winner) return what;
  const winnerName = snap.winner === "white" ? snap.white : snap.black;
  const you =
    snap.myColor === null ? "" : snap.winner === snap.myColor ? " — you won" : " — you lost";
  return `${what} · ${winnerName} wins${you}`;
}
