/**
 * Principal-variation + SAN-line helpers (run-9 field reports round 2).
 *
 * Pure chessops logic shared by the live-analysis strip (UCI PV → SAN
 * line, "Add as variation") and the variation-preview machinery
 * (lib/preview.ts): UCI→SAN conversion, compact move numbering, and
 * SAN-line replay to per-ply FENs. No DOM, no Tauri.
 */
import { Chess, normalizeMove, type Position } from "chessops/chess";
import { makeFen, parseFen } from "chessops/fen";
import { makeSanAndPlay, parseSan } from "chessops/san";
import { makeUci, opposite, parseUci } from "chessops/util";

/** Display cap for the live strip's engine line (plies; full in tooltip). */
export const PV_DISPLAY_PLIES = 8;
/** Insertion cap for "Add as variation" (plies of the PV that are kept). */
export const PV_INSERT_PLIES = 10;

function positionFromFen(fen: string): Position | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const pos = Chess.fromSetup(setup.unwrap());
  return pos.isErr ? null : pos.unwrap();
}

/** Side-to-move flip (chessops has no null move); null when not legal. */
function playNull(pos: Position): Position | null {
  const setup = pos.toSetup();
  setup.turn = opposite(setup.turn);
  setup.epSquare = undefined;
  const r = Chess.fromSetup(setup);
  return r.isOk ? r.unwrap() : null;
}

/**
 * Convert a UCI principal variation to SAN moves by replaying from `fen`.
 * Returns the legal prefix (stops at the first illegal/unparseable move);
 * empty when the FEN itself is bad. Castling arrives as "e1g1" — it is
 * normalized to chessops' internal encoding before legality-checking.
 */
export function uciPvToSan(fen: string, pvUci: string[]): string[] {
  const pos = positionFromFen(fen);
  if (!pos) return [];
  const sans: string[] = [];
  for (const uci of pvUci) {
    const parsed = parseUci(uci);
    if (!parsed) break;
    const move = "from" in parsed ? normalizeMove(pos, parsed) : parsed;
    if (!pos.isLegal(move)) break;
    sans.push(makeSanAndPlay(pos, move));
  }
  return sans;
}

/**
 * Number a SAN line compactly from `fen`'s move number and side to move:
 * "14.Qg3 dxe5 15.fxe5 Nh5", black-to-move start "14...Nh5 15.Qg3".
 * With `maxPlies`, the line is capped and "…" appended when truncated.
 * Pure string arithmetic — the SANs are trusted (already legality-checked).
 */
export function numberSanLine(fen: string, sans: string[], maxPlies?: number): string {
  const parts = fen.split(" ");
  let white = (parts[1] ?? "w") !== "b";
  let num = parseInt(parts[5] ?? "1", 10) || 1;
  const shown = maxPlies !== undefined ? sans.slice(0, maxPlies) : sans;
  const out: string[] = [];
  for (let i = 0; i < shown.length; i++) {
    if (white) {
      out.push(`${num}.${shown[i]}`);
    } else {
      out.push(i === 0 ? `${num}...${shown[i]}` : shown[i]);
      num += 1;
    }
    white = !white;
  }
  if (shown.length < sans.length) out.push("…");
  return out.join(" ");
}

export interface SanLineReplay {
  /** fens[0] is the (normalized) start; fens[k] is after sans[k-1]. */
  fens: string[];
  /** The legal prefix of the input that was actually replayed. */
  sans: string[];
  /** UCI per replayed san; null for null moves ("--"). */
  ucis: (string | null)[];
}

/**
 * Replay a SAN line from `fen`, collecting per-ply FENs and UCIs. Stops
 * at the first illegal move (the returned arrays cover the legal prefix).
 * Null moves ("--", as stored by the annotation tokens) flip the side to
 * move and yield a null uci. Empty result when the FEN is bad.
 */
export function replaySanLine(fen: string, sans: string[]): SanLineReplay {
  let pos = positionFromFen(fen);
  if (!pos) return { fens: [], sans: [], ucis: [] };
  const fens = [makeFen(pos.toSetup())];
  const outSans: string[] = [];
  const ucis: (string | null)[] = [];
  for (const san of sans) {
    if (san === "--") {
      const next = playNull(pos);
      if (!next) break;
      pos = next;
      ucis.push(null);
    } else {
      const move = parseSan(pos, san);
      if (!move) break;
      ucis.push(makeUci(move));
      pos.play(move);
    }
    outSans.push(san);
    fens.push(makeFen(pos.toSetup()));
  }
  return { fens, sans: outSans, ucis };
}
