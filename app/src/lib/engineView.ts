/**
 * Pure display model for UCI engine output.
 *
 * The Rust shell (src-tauri/src/uci.rs) parses raw `info` lines into a
 * structured payload; this module turns that payload into human-readable
 * strings (white-POV eval, SAN principal variation). No DOM, no Tauri.
 */
import { parseFen, makeFen } from "chessops/fen";
import { Chess } from "chessops/chess";
import { makeSanAndPlay } from "chessops/san";
import { parseUci } from "chessops/util";

/** Mirrors the serde struct `UciInfo` emitted by src-tauri/src/uci.rs. */
export interface EngineInfo {
  depth?: number;
  seldepth?: number;
  multipv?: number;
  /** Centipawns from the side-to-move's point of view (UCI convention). */
  scoreCp?: number;
  /** Mate in N moves, side-to-move POV; negative = getting mated. */
  scoreMate?: number;
  nodes?: number;
  nps?: number;
  timeMs?: number;
  /** Principal variation as UCI moves. */
  pv?: string[];
}

/** Terminal payload of an analysis run. */
export interface EngineDone {
  bestmove?: string;
  ponder?: string;
  /** Set when the run ended abnormally (engine died, bad position, ...). */
  error?: string;
}

const whiteToMove = (fen: string): boolean => fen.split(" ")[1] !== "b";

/**
 * Format a score from white's point of view, e.g. "+0.53", "-1.20", "#5", "#-3".
 * `fen` is the analyzed position (UCI scores are side-to-move relative).
 */
export function formatScore(info: EngineInfo, fen: string): string {
  const sign = whiteToMove(fen) ? 1 : -1;
  if (info.scoreMate !== undefined) {
    const m = info.scoreMate * sign;
    return m >= 0 ? `#${m}` : `#-${-m}`;
  }
  if (info.scoreCp !== undefined) {
    const pawns = (info.scoreCp * sign) / 100;
    return `${pawns >= 0 ? "+" : ""}${pawns.toFixed(2)}`;
  }
  return "…";
}

/**
 * Convert a UCI principal variation to a numbered SAN line starting from `fen`,
 * e.g. "1. e4 e5 2. Nf3". Stops at the first illegal move. Returns the raw
 * UCI moves joined by spaces if the FEN itself cannot be parsed.
 */
export function pvToSan(fen: string, pvUci: string[]): string {
  const setup = parseFen(fen);
  if (setup.isErr) return pvUci.join(" ");
  const posRes = Chess.fromSetup(setup.unwrap());
  if (posRes.isErr) return pvUci.join(" ");
  const pos = posRes.unwrap();
  const parts: string[] = [];
  let moveNum = pos.fullmoves;
  let first = true;
  for (const uci of pvUci) {
    const move = parseUci(uci);
    if (!move || !pos.isLegal(move)) break;
    const white = pos.turn === "white";
    if (white) {
      parts.push(`${moveNum}.`);
    } else if (first) {
      parts.push(`${moveNum}...`);
    }
    parts.push(makeSanAndPlay(pos, move));
    if (!white) moveNum += 1;
    first = false;
  }
  return parts.join(" ");
}

/** One-line summary of an info payload, e.g. "d24 +0.53 12.3 Mnodes". */
export function summarizeInfo(info: EngineInfo, fen: string): string {
  const bits: string[] = [];
  if (info.depth !== undefined) bits.push(`d${info.depth}`);
  bits.push(formatScore(info, fen));
  if (info.nodes !== undefined) bits.push(`${(info.nodes / 1e6).toFixed(1)} Mnodes`);
  if (info.nps !== undefined) bits.push(`${(info.nps / 1e6).toFixed(1)} Mn/s`);
  return bits.join("  ");
}

/** Re-export used by tests to sanity-check FEN round-trips. */
export const normalizeFen = (fen: string): string | undefined => {
  const setup = parseFen(fen);
  return setup.isErr ? undefined : makeFen(setup.unwrap());
};
