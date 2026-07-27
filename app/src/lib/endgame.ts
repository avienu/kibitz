/**
 * Endgame trainer: typed IPC wrappers over src-tauri/src/endgame.rs plus
 * pure board helpers (chessops) — no DOM in the pure half, so it is
 * unit-testable in isolation.
 *
 * The user always plays the side to move of the drill FEN; the opponent
 * (tablebase where covered, documented heuristic otherwise) replies inside
 * the same `endgame_move` IPC call.
 */
import { invoke } from "@tauri-apps/api/core";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { parseFen } from "chessops/fen";
import { makeUci, parseSquare, squareRank } from "chessops/util";

/* ---- IPC types (src-tauri/src/endgame.rs, camelCase serde) ---- */

export type Goal = "win" | "draw";
export type Side = "white" | "black";

export interface Tier {
  id: string;
  name: string;
  ratingBand: string;
  summary: string;
}

export interface DrillInfo {
  id: string;
  tier: string;
  title: string;
  concept: string;
  material: string;
  fen: string;
  goal: Goal;
  instruction: string;
  attempts: number;
  solved: number;
  cleanStreak: number;
  mastered: boolean;
}

export interface TbInfo {
  available: boolean;
  largest: number | null;
  note: string;
}

export interface Overview {
  tiers: Tier[];
  drills: DrillInfo[];
  masteryStreak: number;
  tablebase: TbInfo;
}

export interface StartedDrill {
  drillId: string;
  title: string;
  instruction: string;
  goal: Goal;
  fen: string;
  userSide: Side;
  opponentTablebase: boolean;
}

export interface Outcome {
  solved: boolean;
  detail: string;
}

export interface OpponentMove {
  uci: string;
  source: "tablebase" | "heuristic";
}

export interface DrillProgress {
  drillId: string;
  attempts: number;
  solved: number;
  cleanStreak: number;
  mastered: boolean;
}

/**
 * Grading of one move in the feedback aside. User moves are graded ONLY
 * against tablebase truth — never an engine score; `engine` labels the
 * scripted defender's reply row; `unverified` means no tablebase coverage
 * (graded on terminals only).
 */
export type Verdict = "winning" | "slower" | "throws" | "unverified" | "engine";

/** One feedback-aside row: `no | SAN | verdict | note`. */
export interface VerdictRow {
  /** 1-based over the whole session (user moves and replies). */
  index: number;
  san: string;
  verdict: Verdict;
  /** Only for `slower`: plies the tablebase path grew vs the fastest move. */
  dtzCost?: number;
  /** Short human note; empty when the verdict speaks for itself. */
  note: string;
}

export interface MoveResponse {
  fenAfterUser: string;
  opponent: OpponentMove | null;
  fenAfterOpponent: string | null;
  /** Feedback rows ADDED by this step (user row, then the reply's
   * `engine` row when there was one); the client accumulates them. */
  rows: VerdictRow[];
  outcome: Outcome | null;
  /** Set when this move ended the drill (attempt recorded server-side). */
  progress: DrillProgress | null;
}

/* ---- IPC wrappers ---- */

export function endgameOverview(): Promise<Overview> {
  return invoke<Overview>("endgame_overview");
}

export function endgameStart(drillId: string): Promise<StartedDrill> {
  return invoke<StartedDrill>("endgame_start", { drillId });
}

export function endgameMove(uci: string): Promise<MoveResponse> {
  return invoke<MoveResponse>("endgame_move", { uci });
}

export function endgameGiveUp(): Promise<{ progress: DrillProgress }> {
  return invoke<{ progress: DrillProgress }>("endgame_give_up");
}

/* ---- Pure helpers ---- */

/**
 * Turn a chessground drag on `fen` into a UCI move, or null when the drag
 * is not a legal move. Pawn drags onto the last rank promote to the role
 * chosen in the promotion picker (queen when the caller bypasses it).
 */
export function uciForDrag(
  fen: string,
  orig: string,
  dest: string,
  promoRole?: "queen" | "rook" | "bishop" | "knight",
): string | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return null;
  const pos = p.unwrap();
  const from = parseSquare(orig);
  const to = parseSquare(dest);
  if (from === undefined || to === undefined) return null;
  // Promotion role comes from the picker overlay (run-6 item 3); queen is
  // only the fallback for callers that bypass it.
  const promotion =
    pos.board.get(from)?.role === "pawn" && (squareRank(to) === 0 || squareRank(to) === 7)
      ? (promoRole ?? "queen")
      : undefined;
  const move = normalizeMove(pos, { from, to, promotion });
  if (!pos.isLegal(move)) return null;
  return makeUci(move);
}

/** Legal-destination map for the side to move of `fen` (empty when the
 * FEN fails to parse or it is not `side`'s turn). */
export function destsFor(fen: string, side: Side): Map<string, string[]> {
  const setup = parseFen(fen);
  if (setup.isErr) return new Map();
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return new Map();
  const pos = p.unwrap();
  if (pos.turn !== side) return new Map();
  return chessgroundDests(pos);
}

/** "Win with White" / "Hold the draw with Black" — the drill's task line. */
export function goalText(goal: Goal, side: Side): string {
  const color = side === "white" ? "White" : "Black";
  return goal === "win" ? `Win with ${color}` : `Hold the draw with ${color}`;
}

/** Mastery display: "mastered", "1/2 clean", or "" before any progress. */
export function masteryLabel(cleanStreak: number, masteryStreak: number, mastered: boolean): string {
  if (mastered) return "mastered";
  if (cleanStreak > 0) return `${cleanStreak}/${masteryStreak} clean`;
  return "";
}

/** [from, to] of a UCI move for last-move board highlights. */
export function lastMoveOf(uci: string): [string, string] | undefined {
  return uci.length >= 4 ? [uci.slice(0, 2), uci.slice(2, 4)] : undefined;
}
