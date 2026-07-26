/**
 * NAG glyph rendering (run-4 verdict 2).
 *
 * Standard NAGs render as their conventional glyphs. Anything outside the
 * map renders as a small dotted-underline marker with an explanatory
 * tooltip — NEVER as raw "$N" text. NAG 201 (SCID's diagram marker,
 * common in imported databases) renders as nothing visible except a
 * tooltip. Pure data + functions, unit-testable in isolation.
 */

/** NAG number -> display glyph, for every NAG the UI renders directly. */
export const NAG_GLYPHS: Record<number, string> = {
  1: "!",
  2: "?",
  3: "!!",
  4: "??",
  5: "!?",
  6: "?!",
  7: "□", // forced move
  10: "=",
  13: "∞",
  14: "⩲",
  15: "⩱",
  16: "±",
  17: "∓",
  18: "+−",
  19: "−+",
  22: "⨀", // zugzwang
  23: "⨀",
  32: "⟳", // development lead
  33: "⟳",
  36: "↑", // initiative
  40: "↑", // attack
  44: "∞=", // compensation
  132: "⇆", // counterplay
  133: "⇆",
  146: "N", // novelty
};

/** SCID's "insert a diagram here" marker, meaningless as an assessment. */
export const NAG_DIAGRAM = 201;

export interface NagView {
  /** Text to render (empty for the invisible diagram marker). */
  glyph: string;
  /** Tooltip, present for unknown codes and the diagram marker. */
  title?: string;
  /** Render as the small dotted-underline unknown-code marker. */
  unknown: boolean;
  /** Render nothing visible (tooltip only). */
  hidden: boolean;
}

/** How to render NAG `n` in the move list. */
export function nagView(n: number): NagView {
  if (n === NAG_DIAGRAM) {
    return { glyph: "", title: "diagram marker (imported)", unknown: false, hidden: true };
  }
  const glyph = NAG_GLYPHS[n];
  if (glyph !== undefined) {
    return { glyph, unknown: false, hidden: false };
  }
  return { glyph: "·", title: `annotation code $${n}`, unknown: true, hidden: false };
}
