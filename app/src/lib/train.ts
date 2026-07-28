/**
 * Pure helpers for the Repertoire Trainer (Train tab): SAN judging of a
 * board move against a card's expected move, chessground wiring, and
 * interval formatting. No Tauri imports — unit-testable in vitest.
 */
import { Chess, normalizeMove } from "chessops/chess";
import type { NormalMove } from "chessops/types";
import { chessgroundDests } from "chessops/compat";
import { makeFen, parseFen } from "chessops/fen";
import { makeSan, parseSan } from "chessops/san";
import { makeSquare, parseSquare, squareFile, squareRank } from "chessops/util";

/** Parse a FEN into a chessops position (null when invalid). */
export function positionFromFen(fen: string): Chess | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const pos = Chess.fromSetup(setup.unwrap());
  return pos.isErr ? null : pos.unwrap();
}

/** Side to move of `fen`, or null when the FEN is invalid. */
export function turnOf(fen: string): "white" | "black" | null {
  return positionFromFen(fen)?.turn ?? null;
}

/** Legal chessground dests for `fen` (empty map when invalid). */
export function trainDests(fen: string): Map<string, string[]> {
  const pos = positionFromFen(fen);
  return pos ? chessgroundDests(pos) : new Map();
}

/**
 * SAN of the move `orig`→`dest` in `fen`, or null when illegal. Castling
 * is accepted in both king-two-squares and king-onto-rook input forms
 * (same normalization as the game view). Pawns auto-promote to a queen.
 */
export function sanForBoardMove(
  fen: string,
  orig: string,
  dest: string,
  promoRole?: "queen" | "rook" | "bishop" | "knight",
): string | null {
  const pos = positionFromFen(fen);
  if (!pos) return null;
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
  return makeSan(pos, move);
}

/** Strip check/mate/annotation suffixes so `Nf3+!?` matches `Nf3`. */
function normSan(san: string): string {
  return san.replace(/[+#!?]+$/, "");
}

/** Does the played SAN answer the card's expected SAN? */
export function sanMatches(expected: string, played: string): boolean {
  return normSan(expected) === normSan(played);
}

/**
 * Board arrow for the expected SAN in `fen` (shown after a wrong answer).
 * Castling arrows point at the king's destination square (g/c file), not
 * at the rook chessops encodes internally.
 */
export function expectedArrow(fen: string, san: string): { orig: string; dest: string } | null {
  const pos = positionFromFen(fen);
  if (!pos) return null;
  const move = parseSan(pos, normSan(san)) as NormalMove | undefined;
  if (!move || move.from === undefined) return null;
  let to = move.to;
  const isCastle = pos.board.get(move.from)?.role === "king" && pos.board.get(move.to)?.role === "rook";
  if (isCastle) {
    const rank = squareRank(move.from);
    const kingside = squareFile(move.to) > squareFile(move.from);
    to = rank * 8 + (kingside ? 6 : 2);
  }
  return { orig: makeSquare(move.from), dest: makeSquare(to) };
}

/** FEN after playing `san` in `fen` (null when invalid/illegal). */
export function fenAfterSan(fen: string, san: string): string | null {
  const pos = positionFromFen(fen);
  if (!pos) return null;
  const move = parseSan(pos, normSan(san));
  if (!move) return null;
  pos.play(move);
  return makeFen(pos.toSetup());
}

/** Human-readable review interval: "<1d", "13d", "3mo", "1.5y". */
export function formatInterval(days: number): string {
  if (days < 1) return "<1d";
  if (days < 45) return `${Math.round(days)}d`;
  if (days < 365) return `${Math.round(days / 30.44)}mo`;
  return `${(days / 365.25).toFixed(1)}y`;
}

/* ---- Openings SRS keyboard map (design/handoff-2 §Interactions) ---- */

export type SrsKeyAction =
  | "grade-again"
  | "grade-hard"
  | "grade-good"
  | "grade-easy"
  | "submit";

const GRADE_KEYS: Record<string, SrsKeyAction> = {
  "1": "grade-again",
  "2": "grade-hard",
  "3": "grade-good",
  "4": "grade-easy",
};

/**
 * Window-level key handling for an SRS session: `1–4` grade AFTER the
 * answer is revealed, `⏎` submits the typed move before it. Keys never
 * fire while a text input is focused (`editable` — the SAN field handles
 * its own Enter) or while a modifier is held.
 */
export function srsKeyAction(
  key: string,
  opts: { editable: boolean; revealed: boolean; modifier?: boolean },
): SrsKeyAction | null {
  if (opts.modifier || opts.editable) return null;
  if (key === "Enter") return opts.revealed ? null : "submit";
  if (!opts.revealed) return null;
  return GRADE_KEYS[key] ?? null;
}

/**
 * Which colour toggle the Openings SRS screen should open on (audit
 * 2026-07 #6): the colour that actually has due cards. Prefer a colour
 * with due > 0; when both (or neither) have due cards, White. Applied on
 * screen entry only — an explicit user toggle is never overridden.
 */
export function defaultTrainColor(counts: {
  white: { due: number };
  black: { due: number };
}): "white" | "black" {
  return counts.black.due > 0 && counts.white.due === 0 ? "black" : "white";
}

/** Outcome tallies for the end-of-session summary. */
export interface SessionSummary {
  reviewed: number;
  correct: number;
  again: number;
}

export function emptySummary(): SessionSummary {
  return { reviewed: 0, correct: 0, again: 0 };
}

export function tallyAnswer(s: SessionSummary, correct: boolean): SessionSummary {
  return {
    reviewed: s.reviewed + 1,
    correct: s.correct + (correct ? 1 : 0),
    again: s.again + (correct ? 0 : 1),
  };
}
