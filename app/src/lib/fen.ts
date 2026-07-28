/**
 * Tiny FEN field helpers. The board widget needs the side to move WITHOUT
 * a full chessops parse (it runs on every position update), and getting it
 * wrong is invisible until a user's click silently becomes a premove —
 * see the run-9 audit: chessground's turnColor defaults to "white", so
 * every Black-to-move board swallowed moves as queued premoves.
 */

/** Side to move from a FEN string ("w"/"b" second field; defaults white). */
export function fenTurn(fen: string): "white" | "black" {
  return fen.split(" ")[1] === "b" ? "black" : "white";
}
