/**
 * Position-search helpers (design/handoff-2 §Position search): building a
 * searchable FEN out of the drag-to-set-up board editor. Pure logic.
 *
 * The editor manipulates piece placement only; side to move comes from a
 * toggle. Castling rights are granted exactly where a king and rook still
 * stand on their home squares (the standard editor heuristic — stated in
 * the screen's hint line), en passant is none.
 */

/** Piece on a square, per the FEN placement field (ranks 8 → 1). */
function pieceAt(placement: string, square: string): string | null {
  const file = square.charCodeAt(0) - 97; // a..h
  const rank = Number(square[1]); // 1..8
  const rows = placement.split("/");
  if (rows.length !== 8) return null;
  const row = rows[8 - rank];
  let f = 0;
  for (const ch of row) {
    if (/\d/.test(ch)) {
      f += Number(ch);
    } else {
      if (f === file) return ch;
      f++;
    }
    if (f > file) break;
  }
  return null;
}

/** Castling-rights field derived from king/rook home squares ("-" if none). */
export function castlingFromPlacement(placement: string): string {
  let rights = "";
  if (pieceAt(placement, "e1") === "K") {
    if (pieceAt(placement, "h1") === "R") rights += "K";
    if (pieceAt(placement, "a1") === "R") rights += "Q";
  }
  if (pieceAt(placement, "e8") === "k") {
    if (pieceAt(placement, "h8") === "r") rights += "k";
    if (pieceAt(placement, "a8") === "r") rights += "q";
  }
  return rights === "" ? "-" : rights;
}

/** Full FEN from an edited placement + side to move (no ep, counters reset). */
export function fenFromPlacement(placement: string, turn: "white" | "black"): string {
  return `${placement} ${turn === "white" ? "w" : "b"} ${castlingFromPlacement(placement)} - 0 1`;
}

/** The placement field of a full FEN (chessground's getFen shape). */
export function placementOf(fen: string): string {
  return fen.split(" ")[0];
}

/** The side-to-move field of a full FEN (defaults to white). */
export function turnOf(fen: string): "white" | "black" {
  return fen.split(" ")[1] === "b" ? "black" : "white";
}
