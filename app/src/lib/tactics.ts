/**
 * Tactics trainer: typed IPC wrappers over src-tauri/src/tactics.rs plus
 * the pure puzzle solve-model (chessops) — no DOM in the pure half, so it
 * is unit-testable in isolation.
 *
 * Lichess puzzle convention: `moves[0]` is the opponent's setup move
 * played from `fen`; the user solves every move at ODD indices, and the
 * app auto-plays the opponent replies at even indices.
 */
import { invoke } from "@tauri-apps/api/core";
import { Chess } from "chessops/chess";
import { makeFen, parseFen } from "chessops/fen";
import { makeSanAndPlay } from "chessops/san";
import { parseUci } from "chessops/util";
import type { PlayerProfile } from "./db";

/* ---- IPC types (src-tauri/src/tactics.rs, camelCase serde) ---- */

export interface PuzzleRow {
  id: number;
  lichessId: string;
  fen: string;
  /** UCI line; index 0 is the opponent's setup move. */
  moves: string[];
  rating: number;
  popularity: number;
  themes: string[];
}

export interface ServedPuzzle {
  puzzle: PuzzleRow;
  /** Weakness mode: dominant profiled motif this puzzle trains. */
  motif?: string;
  /** Weakness mode: UI-ready explanation of why this puzzle was chosen. */
  reason?: string;
  matchedThemes: string[];
  allowed: number;
  missed: number;
}

export interface ThemeCount {
  theme: string;
  puzzles: number;
}

export interface TacticsState {
  rating: number;
  attempts: number;
  puzzles: number;
  themes: ThemeCount[];
}

export interface AttemptOutcome {
  ratingBefore: number;
  ratingAfter: number;
  attempts: number;
}

export interface ImportPuzzlesSummary {
  imported: number;
  duplicatesSkipped: number;
  filteredOut: number;
  malformed: number;
  elapsedMs: number;
}

export interface WoodpeckerSet {
  id: number;
  name: string;
  size: number;
  cycles: number;
  createdAt: string;
}

export interface CycleStats {
  cycleId: number;
  cycleNo: number;
  startedAt: string;
  finishedAt: string | null;
  attempts: number;
  solved: number;
  accuracyPct: number;
  totalTimeMs: number;
  avgTimeMs: number;
}

export interface MotifWeight {
  kind: string;
  allowed: number;
  missed: number;
}

export type DrillMode = "rated" | "motif" | "weakness" | "woodpecker" | "speed";
export type Verdict = "correct" | "correctAltMate" | "wrong";

/* ---- IPC wrappers ---- */

export function tacticsState(): Promise<TacticsState> {
  return invoke<TacticsState>("tactics_state");
}

export function importPuzzles(
  path: string,
  minPopularity?: number,
  maxRows?: number,
): Promise<ImportPuzzlesSummary> {
  return invoke<ImportPuzzlesSummary>("tactics_import_puzzles", {
    path,
    minPopularity: minPopularity ?? null,
    maxRows: maxRows ?? null,
  });
}

export function nextPuzzle(
  mode: "rated" | "motif" | "weakness" | "speed",
  theme?: string,
  weights?: MotifWeight[],
): Promise<ServedPuzzle | null> {
  return invoke<ServedPuzzle | null>("tactics_next_puzzle", {
    mode,
    theme: theme ?? null,
    weights: weights ?? null,
  });
}

export function verifyMove(fen: string, expected: string, played: string): Promise<Verdict> {
  return invoke<Verdict>("tactics_verify_move", { fen, expected, played });
}

export function recordAttempt(
  puzzleId: number,
  solved: boolean,
  timeMs: number,
  mode: DrillMode,
  cycleId?: number,
): Promise<AttemptOutcome> {
  return invoke<AttemptOutcome>("tactics_record_attempt", {
    puzzleId,
    solved,
    timeMs,
    mode,
    cycleId: cycleId ?? null,
  });
}

export function woodpeckerSets(): Promise<WoodpeckerSet[]> {
  return invoke<WoodpeckerSet[]>("tactics_woodpecker_sets");
}

export function createWoodpeckerSet(name: string, size: number): Promise<number> {
  return invoke<number>("tactics_create_woodpecker_set", { name, size });
}

export function woodpeckerPuzzles(setId: number): Promise<PuzzleRow[]> {
  return invoke<PuzzleRow[]>("tactics_woodpecker_puzzles", { setId });
}

export function startCycle(setId: number): Promise<number> {
  return invoke<number>("tactics_start_cycle", { setId });
}

export function finishCycle(cycleId: number): Promise<void> {
  return invoke<void>("tactics_finish_cycle", { cycleId });
}

export function cycleStats(setId: number): Promise<CycleStats[]> {
  return invoke<CycleStats[]>("tactics_cycle_stats", { setId });
}

/* ---- Pure solve model ---- */

/** The user's motif rows, in the shape the weakness selector consumes. */
export function motifWeightsFromProfile(profile: PlayerProfile): MotifWeight[] {
  return profile.motifs.map((m) => ({ kind: m.kind, allowed: m.allowed, missed: m.missed }));
}

export interface PuzzleModel {
  /** fens[i] = position after i moves of the line (fens[0] = puzzle FEN). */
  fens: string[];
  /** SAN of move i, for solution display. */
  sans: string[];
  /** [from, to] of move i, for last-move highlights. */
  lastMoves: [string, string][];
  /** The side the USER plays (the setup move belongs to the opponent). */
  solverColor: "white" | "black";
}

/**
 * Precompute the full line of a puzzle. Returns null when the FEN or any
 * stored move fails to parse/play (corrupt data — the caller skips it).
 */
export function buildPuzzleModel(fen: string, ucis: string[]): PuzzleModel | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return null;
  const pos = p.unwrap();
  const solverColor = pos.turn === "white" ? "black" : "white";
  const fens = [makeFen(pos.toSetup())];
  const sans: string[] = [];
  const lastMoves: [string, string][] = [];
  for (const uci of ucis) {
    const move = parseUci(uci);
    if (!move || !("from" in move) || !pos.isLegal(move)) return null;
    lastMoves.push(uci.length >= 4 ? [uci.slice(0, 2), uci.slice(2, 4)] : ["a1", "a1"]);
    sans.push(makeSanAndPlay(pos, move));
    fens.push(makeFen(pos.toSetup()));
  }
  return { fens, sans, lastMoves, solverColor };
}

/** True when line index i (0-based) is a move the user must find. */
export function isSolverMove(i: number): boolean {
  return i % 2 === 1;
}

/** "3:05" style clock for drill timers. */
export function formatClock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
