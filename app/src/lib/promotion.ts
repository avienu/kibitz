/**
 * Promotion detection (run-6 item 3): the board input layer auto-queened
 * everywhere; every surface that accepts moves now routes pawn moves to
 * the last rank through a promotion picker instead.
 *
 * Pure logic — no DOM, no Tauri — unit-testable in isolation.
 */
import { Chess, normalizeMove } from "chessops/chess";
import { parseFen } from "chessops/fen";
import { parseSquare, squareRank } from "chessops/util";

export type PromoRole = "queen" | "rook" | "bishop" | "knight";

/** Picker order; also the 1–4 key bindings. */
export const PROMO_ROLES: readonly PromoRole[] = ["queen", "rook", "bishop", "knight"];

export const PROMO_UCI: Record<PromoRole, string> = {
  queen: "q",
  rook: "r",
  bishop: "b",
  knight: "n",
};

/** Unicode glyphs for the picker buttons, per side. */
export const PROMO_GLYPHS: Record<"white" | "black", Record<PromoRole, string>> = {
  white: { queen: "♕", rook: "♖", bishop: "♗", knight: "♘" },
  black: { queen: "♛", rook: "♜", bishop: "♝", knight: "♞" },
};

/** Digit-key (1–4) → role, per the picker overlay's key map. */
export function promoKeyRole(key: string): PromoRole | null {
  const i = ["1", "2", "3", "4"].indexOf(key);
  return i === -1 ? null : PROMO_ROLES[i];
}

/**
 * If dragging `orig`→`dest` on `fen` is a legal pawn promotion, return
 * the moving side (the picker needs its glyph colour); else null.
 */
export function promotionPending(
  fen: string,
  orig: string,
  dest: string,
): "white" | "black" | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const p = Chess.fromSetup(setup.unwrap());
  if (p.isErr) return null;
  const pos = p.unwrap();
  const from = parseSquare(orig);
  const to = parseSquare(dest);
  if (from === undefined || to === undefined) return null;
  if (pos.board.get(from)?.role !== "pawn") return null;
  if (squareRank(to) !== 0 && squareRank(to) !== 7) return null;
  const move = normalizeMove(pos, { from, to, promotion: "queen" });
  return pos.isLegal(move) ? pos.turn : null;
}
