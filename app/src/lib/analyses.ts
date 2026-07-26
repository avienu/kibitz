/**
 * Per-ply engine evaluation view model (run-4 verdict 3c).
 *
 * The `game_analyses` command returns raw rows from the analyses table.
 * POV convention: 'fresh' rows are side-to-move POV at their ply (after
 * `ply` plies White is to move iff ply is even, so White-POV = negate at
 * odd plies); 'legacy-import' rows are already White-POV (SCID
 * convention). The move list displays White-POV consistently, preferring
 * fresh rows over legacy imports at the same ply.
 *
 * No DOM, no Tauri — unit-testable in isolation.
 */

/** Wire shape of one `game_analyses` row (src-tauri/src/dbops.rs). */
export interface AnalysisRow {
  /** Position after `ply` mainline plies (1-based). */
  ply: number;
  /** "fresh" | "legacy-import". */
  kind: string;
  engine: string;
  depth: number | null;
  nodes: number | null;
  evalCp: number;
  createdAt: string;
}

/** The one evaluation the move list shows for a ply. */
export interface PlyEval {
  /** Centipawns, White's point of view. */
  whiteCp: number;
  kind: "fresh" | "legacy";
  engine: string;
}

/**
 * Convert a stored eval to White-POV centipawns. Fresh rows are
 * side-to-move POV: negate when the ply is odd (Black to move after an
 * odd number of plies). Legacy imports are already White-POV.
 */
export function whitePovCp(kind: string, ply: number, evalCp: number): number {
  return kind === "fresh" && ply % 2 === 1 ? -evalCp : evalCp;
}

/**
 * Pick the display eval per ply: fresh beats legacy; within a kind the
 * first row wins (the command orders newest first).
 */
export function evalsByPly(rows: AnalysisRow[]): Map<number, PlyEval> {
  const map = new Map<number, PlyEval>();
  for (const r of rows) {
    const kind = r.kind === "fresh" ? "fresh" : "legacy";
    const existing = map.get(r.ply);
    if (existing && (existing.kind === "fresh" || kind === "legacy")) continue;
    map.set(r.ply, { whiteCp: whitePovCp(r.kind, r.ply, r.evalCp), kind, engine: r.engine });
  }
  return map;
}

/** "+0.4" / "-1.2" style pawn-unit display of a White-POV cp value. */
export function formatWhiteCp(cp: number): string {
  const v = cp / 100;
  const s = v.toFixed(1);
  return v >= 0 ? `+${s}` : s;
}

/** Tooltip for a legacy-import eval (verdict 3c wording). */
export function legacyEvalTitle(engine: string): string {
  return `Engine Vintage: ${engine}, imported analysis`;
}
