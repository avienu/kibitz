/**
 * Pure game model shared by the Analyze (PGN paste) and Database views.
 *
 * Builds a LoadedGame either from PGN text (first game's mainline) or from
 * a SAN array + optional start FEN (as returned by the `get_game` command),
 * precomputing the FEN after every ply so the UI can step through moves
 * with O(1) lookups. No DOM, no Tauri — unit-testable in isolation.
 */
import { Chess, type Position } from "chessops/chess";
import { makeFen, parseFen } from "chessops/fen";
import { parsePgn, startingPosition } from "chessops/pgn";
import { parseSan } from "chessops/san";
import { makeUci } from "chessops/util";

export interface LoadedGame {
  /** Selected PGN headers (White, Black, Event, Result, ...). */
  headers: Record<string, string>;
  /**
   * fens[i] is the position after i plies of the mainline.
   * fens[0] is the initial position; fens[sans.length] is the final position.
   */
  fens: string[];
  /** SAN of ply i (the move played from fens[i]). */
  sans: string[];
  /** UCI of ply i, e.g. "e2e4" — used for last-move board highlights. */
  ucis: string[];
}

export type LoadGameResult =
  | { ok: true; game: LoadedGame; warning?: string }
  | { ok: false; error: string };

/**
 * Play `sans` out from `pos` (mutating it), collecting per-ply FENs/UCIs.
 * Stops at the first illegal move, reporting it as a warning.
 */
function playMainline(
  pos: Position,
  sanMoves: Iterable<string>,
): { fens: string[]; sans: string[]; ucis: string[]; warning?: string } {
  const fens: string[] = [makeFen(pos.toSetup())];
  const sans: string[] = [];
  const ucis: string[] = [];
  let warning: string | undefined;
  for (const san of sanMoves) {
    const move = parseSan(pos, san);
    if (!move) {
      warning = `Illegal or unparseable move "${san}" at ply ${sans.length + 1}; mainline truncated.`;
      break;
    }
    sans.push(san);
    ucis.push(makeUci(move));
    pos.play(move);
    fens.push(makeFen(pos.toSetup()));
  }
  if (sans.length === 0 && !warning) {
    warning = "Game has no moves.";
  }
  return { fens, sans, ucis, warning };
}

/**
 * Parse PGN text and play out the mainline of the first game.
 * Tolerates trailing garbage; stops the mainline at the first illegal move
 * (returned as a warning rather than a hard error).
 */
export function loadGame(pgnText: string): LoadGameResult {
  const games = parsePgn(pgnText);
  if (games.length === 0) {
    return { ok: false, error: "No game found in PGN input." };
  }
  const g = games[0];
  const start = startingPosition(g.headers);
  if (start.isErr) {
    return { ok: false, error: `Bad starting position: ${start.error.message}` };
  }
  const pos = start.unwrap();
  const sanMoves = Array.from(g.moves.mainline(), (node) => node.san);
  const { fens, sans, ucis, warning } = playMainline(pos, sanMoves);
  const headers: Record<string, string> = {};
  for (const [k, v] of g.headers) headers[k] = v;
  return { ok: true, game: { headers, fens, sans, ucis }, warning };
}

/**
 * Build a game from a SAN array + optional start FEN (null/undefined =
 * standard initial position), e.g. from the database `get_game` command.
 * Same tolerance as loadGame: an illegal SAN truncates with a warning.
 */
export function gameFromSans(
  sanMoves: string[],
  startFen?: string | null,
  headers: Record<string, string> = {},
): LoadGameResult {
  let pos: Position;
  if (startFen) {
    const setup = parseFen(startFen);
    if (setup.isErr) {
      return { ok: false, error: `Bad start FEN: ${setup.error.message}` };
    }
    const p = Chess.fromSetup(setup.unwrap());
    if (p.isErr) {
      return { ok: false, error: `Illegal start position: ${p.error.message}` };
    }
    pos = p.unwrap();
  } else {
    pos = Chess.default();
  }
  const { fens, sans, ucis, warning } = playMainline(pos, sanMoves);
  return { ok: true, game: { headers, fens, sans, ucis }, warning };
}

/** Clamp a requested ply into the valid range [0, plies]. */
export function clampPly(ply: number, game: LoadedGame): number {
  return Math.max(0, Math.min(game.sans.length, ply));
}

/** chessground last-move highlight for the position at `ply` (after ply moves). */
export function lastMoveAt(game: LoadedGame, ply: number): [string, string] | undefined {
  if (ply <= 0 || ply > game.ucis.length) return undefined;
  const uci = game.ucis[ply - 1];
  return [uci.slice(0, 2), uci.slice(2, 4)];
}

/** Move list annotated with move numbers, e.g. ["1. e4", "1... e5", "2. Nf3"]. */
export function numberedSans(game: LoadedGame): string[] {
  const startFen = game.fens[0];
  const parts = startFen.split(" ");
  const startMoveNum = parseInt(parts[5] ?? "1", 10) || 1;
  let whiteToMove = (parts[1] ?? "w") === "w";
  let moveNum = startMoveNum;
  const out: string[] = [];
  for (const san of game.sans) {
    out.push(whiteToMove ? `${moveNum}. ${san}` : `${moveNum}... ${san}`);
    if (!whiteToMove) moveNum += 1;
    whiteToMove = !whiteToMove;
  }
  return out;
}
