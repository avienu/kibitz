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
export function sanForBoardMove(fen: string, orig: string, dest: string): string | null {
  const pos = positionFromFen(fen);
  if (!pos) return null;
  const from = parseSquare(orig);
  const to = parseSquare(dest);
  if (from === undefined || to === undefined) return null;
  const promotion =
    pos.board.get(from)?.role === "pawn" && (squareRank(to) === 0 || squareRank(to) === 7)
      ? ("queen" as const)
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
