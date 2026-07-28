/**
 * Variation preview (run-9 field report: "I can't click into variations").
 *
 * Clicking a variation row in the Moves panel loads that line onto the
 * board in a lightweight preview mode: the board shows the position at
 * the variation's branch point plus the variation moves, stepped with the
 * preview's own controls (or ←/→ while active). The MAIN game's ply is
 * untouched — any mainline navigation, the "back to game" pill, or Esc
 * exits the preview and the board snaps back to where you were.
 *
 * Pure state machine — no DOM, no Tauri. App.tsx owns the state; enter /
 * step are here, exit is simply dropping the state.
 */
import { replaySanLine } from "./pv";

/** What the Moves panel's variation row carries (lib/movesView.ts). */
export interface VariationRowLike {
  /** 1-based mainline ply of the move the variation replaces. */
  branchPly: number;
  /** The variation's own moves (top-level; "--" = null move). */
  sans: string[];
  /** First move with its number, e.g. "14.Qg3" — the pill label. */
  label: string;
  /** Token index of the variation's varStart (row highlight). */
  varStartIndex?: number;
}

export interface VariationPreview {
  /** fens[0] = branch point (position BEFORE the replaced mainline move);
   * fens[k] = after k variation moves. */
  fens: string[];
  /** The replayable prefix of the variation's moves. */
  sans: string[];
  /** UCI per variation move (null for null moves) — board highlight. */
  ucis: (string | null)[];
  branchPly: number;
  varStartIndex: number | null;
  label: string;
  /** 0..sans.length — index into fens (position shown on the board). */
  at: number;
}

/**
 * Enter preview for a variation row, replaying from the branch-point FEN
 * (mainline fens[branchPly - 1]). Opens ON the first variation move —
 * that is what the click asked to see. Null when the variation has no
 * replayable moves (e.g. a comment-only group or an illegal stored line).
 */
export function enterPreview(branchFen: string, row: VariationRowLike): VariationPreview | null {
  if (row.sans.length === 0) return null;
  const { fens, sans, ucis } = replaySanLine(branchFen, row.sans);
  if (sans.length === 0) return null;
  return {
    fens,
    sans,
    ucis,
    branchPly: row.branchPly,
    varStartIndex: row.varStartIndex ?? null,
    label: row.label,
    at: 1,
  };
}

/** Step within the preview, clamped to [0, sans.length]. */
export function stepPreview(p: VariationPreview, delta: number): VariationPreview {
  const at = Math.max(0, Math.min(p.sans.length, p.at + delta));
  return at === p.at ? p : { ...p, at };
}

/** The FEN the board shows while previewing. */
export function previewFen(p: VariationPreview): string {
  return p.fens[p.at];
}

/** Last-move highlight for the shown preview position. */
export function previewLastMove(p: VariationPreview): [string, string] | undefined {
  if (p.at === 0) return undefined;
  const uci = p.ucis[p.at - 1];
  return uci ? [uci.slice(0, 2), uci.slice(2, 4)] : undefined;
}
