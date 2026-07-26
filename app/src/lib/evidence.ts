/**
 * The evidence-overlay language (design/handoff-1/README.md): the ONE shared
 * overlay vocabulary used by Explain, Opponent prep and Profile drill-down.
 *
 * Pure logic — no DOM, no chessground, and deliberately **no treatment or
 * theme inputs**: the same Evidence yields byte-identical output whatever the
 * board skin (walnut/instrument) or theme (dark/light). Evidence hues are
 * semantic and unthemed; the only theme-dependent colours (last-move wash,
 * selected ring) live in CSS, keyed off the mark's role class.
 *
 * One meaning per colour, one shape per role. Never introduce a new colour or
 * shape for a new surface.
 */

/** Arrow/brush roles — also the chessground brush names. */
export type ArrowKind = "alert" | "attacker" | "defender" | "imbalance" | "key";

/** Per-square mark roles (renderer maps each to a CSS class). */
export type EvidenceRole =
  | "alert-target"
  | "attacker"
  | "defender"
  | "imbalance"
  | "key"
  | "last-move"
  | "selected";

export interface EvidenceArrow {
  from: string;
  to: string;
  kind: ArrowKind;
}

/**
 * Evidence input — the backend contract for per-block explanation evidence.
 * Field names are fixed; do not rename.
 */
export interface Evidence {
  /** Alert target squares (ring). */
  alerts: string[];
  /** Attacker squares (corner wedge + arrow). */
  attackers: string[];
  /** Defender squares (corner wedge, no arrow). */
  defenders: string[];
  /** Imbalance-evidence squares (full-square wash). */
  imbalance: string[];
  /** Key squares / plan targets (corner wedge). */
  key: string[];
  /** Arrows, always attacker → target, never the reverse. */
  arrows: EvidenceArrow[];
}

/** One absolutely-positioned square child (ring/wedge/wash), paint-ordered. */
export interface SquareMark {
  square: string;
  role: EvidenceRole;
  /** Element opacity per the intensity rules (1 for last-move/selected). */
  opacity: number;
}

/** Board shape: circle when no dest, arrow orig→dest otherwise. Evidence
 * arrows are rendered by the module's own SVG layer; the same shape objects
 * remain chessground-autoShape-compatible for the legacy trainer path. */
export interface BoardShape {
  orig: string;
  dest?: string;
  brush: string;
}

/** Exact evidence hues — identical in both themes (never themed). */
export const EVIDENCE_COLORS: Readonly<Record<ArrowKind, { line: string; fill: string }>> = {
  alert: { line: "#e05c4b", fill: "rgba(224,92,75,0.20)" },
  attacker: { line: "#e8a13c", fill: "rgba(232,161,60,0.18)" },
  defender: { line: "#4f9ad8", fill: "rgba(79,154,216,0.16)" },
  imbalance: { line: "#5fb08a", fill: "rgba(95,176,138,0.20)" },
  key: { line: "#a98bd4", fill: "rgba(169,139,212,0.20)" },
};

/** Overlays render at 0.44 by default; 1.0 for the hovered Explain sentence. */
export const DEFAULT_INTENSITY = 0.44;

/**
 * Paint order per square, bottom → top (base square is below all marks, the
 * piece above all of them): last-move wash → imbalance → key → defender →
 * attacker → alert ring → selected ring.
 */
export const ROLE_PAINT_ORDER: Readonly<Record<EvidenceRole, number>> = {
  "last-move": 0,
  imbalance: 1,
  key: 2,
  defender: 3,
  attacker: 4,
  "alert-target": 5,
  selected: 6,
};

/** Arrow de-dup priority: first role wins — alert/attacker before key. */
const ARROW_PRIORITY: Readonly<Record<ArrowKind, number>> = {
  alert: 0,
  attacker: 1,
  defender: 2,
  imbalance: 3,
  key: 4,
};

const SQUARE = /^[a-h][1-8]$/;

const isSquare = (sq: unknown): sq is string => typeof sq === "string" && SQUARE.test(sq);

const clamp01 = (n: number): number => Math.min(1, Math.max(0, n));

export interface EvidenceViewOptions {
  /** The single loudness knob (0..1). Default 0.44; 1.0 when hovered. */
  intensity?: number;
  /**
   * Per-block isolation: when set, only evidence marks on these squares (and
   * arrows touching them) are kept. Position state (last move, selection) is
   * not evidence and always renders.
   */
  isolate?: ReadonlySet<string> | null;
  /** Last move [from, to] — merged in as last-move washes. */
  lastMove?: readonly [string, string] | null;
  /** Selected square — merged in as the selected ring. */
  selected?: string | null;
}

/** Everything the board renderer needs; pure data, JSON-serializable. */
export interface EvidenceView {
  intensity: number;
  /** Ring opacity = 0.42 + 0.5 × intensity. */
  ringOpacity: number;
  /** Wash opacity = 0.5 + 0.5 × intensity. */
  washOpacity: number;
  /** Wedge opacity = 0.55 + 0.45 × intensity. */
  wedgeOpacity: number;
  /** Arrow opacity = 0.42 + 0.44 × intensity. */
  arrowOpacity: number;
  /** Square marks, sorted by paint order (bottom → top). */
  marks: SquareMark[];
  /** Arrows (deduped, attacker→target); `brush` names the role/hue. Drawn by
   * the overlay's own SVG layer via arrowPolygonPoints(). */
  shapes: BoardShape[];
}

/**
 * Compute the full overlay view for one position. `evidence` may be null
 * (e.g. only a last move to show); invalid squares are dropped silently.
 */
export function evidenceView(
  evidence: Evidence | null | undefined,
  opts: EvidenceViewOptions = {},
): EvidenceView {
  const intensity = clamp01(opts.intensity ?? DEFAULT_INTENSITY);
  const ringOpacity = 0.42 + 0.5 * intensity;
  const washOpacity = 0.5 + 0.5 * intensity;
  const wedgeOpacity = 0.55 + 0.45 * intensity;
  const arrowOpacity = 0.42 + 0.44 * intensity;
  const isolate = opts.isolate ?? null;

  const marks: SquareMark[] = [];
  const pushRole = (squares: string[] | undefined, role: EvidenceRole, opacity: number) => {
    const seen = new Set<string>();
    for (const sq of squares ?? []) {
      if (!isSquare(sq) || seen.has(sq)) continue;
      if (isolate && !isolate.has(sq)) continue;
      seen.add(sq);
      marks.push({ square: sq, role, opacity });
    }
  };

  // Pushed in paint order (ROLE_PAINT_ORDER), so `marks` is pre-sorted.
  const lastMove = (opts.lastMove ?? []).filter(isSquare);
  for (const sq of new Set(lastMove)) marks.push({ square: sq, role: "last-move", opacity: 1 });
  pushRole(evidence?.imbalance, "imbalance", washOpacity);
  pushRole(evidence?.key, "key", wedgeOpacity);
  pushRole(evidence?.defenders, "defender", wedgeOpacity);
  pushRole(evidence?.attackers, "attacker", wedgeOpacity);
  pushRole(evidence?.alerts, "alert-target", ringOpacity);
  if (isSquare(opts.selected)) marks.push({ square: opts.selected, role: "selected", opacity: 1 });

  // Arrows: stable-sort by role priority, then dedupe by from|to (first wins).
  const shapes: BoardShape[] = [];
  const seenArrow = new Set<string>();
  const arrows = (evidence?.arrows ?? [])
    .filter(
      (a) =>
        isSquare(a.from) &&
        isSquare(a.to) &&
        a.from !== a.to &&
        a.kind in ARROW_PRIORITY &&
        (!isolate || isolate.has(a.from) || isolate.has(a.to)),
    )
    .map((a, i) => ({ a, i }))
    .sort((x, y) => ARROW_PRIORITY[x.a.kind] - ARROW_PRIORITY[y.a.kind] || x.i - y.i);
  for (const { a } of arrows) {
    const key = `${a.from}|${a.to}`;
    if (seenArrow.has(key)) continue;
    seenArrow.add(key);
    shapes.push({ orig: a.from, dest: a.to, brush: a.kind });
  }

  return {
    intensity,
    ringOpacity,
    washOpacity,
    wedgeOpacity,
    arrowOpacity,
    marks,
    shapes,
  };
}

// ---------------------------------------------------------------------------
// Arrow geometry — filled polygons in board-pixel coordinates.
// ---------------------------------------------------------------------------

/** Centre of a square in board pixels, through the same cell mapping the
 * mark grid uses (mirrors when the orientation is black). */
export function squareCenter(
  square: string,
  cell: number,
  orientation: "white" | "black" = "white",
): [number, number] {
  const f = square.charCodeAt(0) - 97; // a..h
  const r = square.charCodeAt(1) - 49; // 1..8
  const x = (orientation === "black" ? 7 - f : f) * cell + cell / 2;
  const y = (orientation === "black" ? r : 7 - r) * cell + cell / 2;
  return [x, y];
}

/**
 * The spec's exact filled-polygon arrow, with u = cell/100: start offset 33u
 * from the source centre, tip stopping 33u short of the target centre, head
 * length 27u, head half-width 17u, shaft half-width 5.2u. Returns the SVG
 * `points` string (one decimal, like the reference renderer).
 */
export function arrowPolygonPoints(
  from: string,
  to: string,
  cell: number,
  orientation: "white" | "black" = "white",
): string {
  const u = cell / 100;
  const [x1, y1] = squareCenter(from, cell, orientation);
  const [x2, y2] = squareCenter(to, cell, orientation);
  let dx = x2 - x1;
  let dy = y2 - y1;
  const len = Math.hypot(dx, dy) || 1;
  dx /= len;
  dy /= len;
  const px = -dy;
  const py = dx;
  const start = 33 * u;
  const headLen = 27 * u;
  const headW = 17 * u;
  const w = 5.2 * u;
  const sx = x1 + dx * start;
  const sy = y1 + dy * start;
  const ex = x2 - dx * 33 * u;
  const ey = y2 - dy * 33 * u;
  const bx = ex - dx * headLen;
  const by = ey - dy * headLen;
  const points: [number, number][] = [
    [sx + px * w, sy + py * w],
    [bx + px * w, by + py * w],
    [bx + px * headW, by + py * headW],
    [ex, ey],
    [bx - px * headW, by - py * headW],
    [bx - px * w, by - py * w],
    [sx - px * w, sy - py * w],
  ];
  return points.map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" ");
}

/** Arrow outline stroke width: max(0.75, cell × 0.016). Stroke colour is
 * themed and lives in CSS (rgba(10,13,15,0.5) dark / rgba(255,255,255,0.5)
 * light). */
export function arrowStrokeWidth(cell: number): number {
  return Math.max(0.75, cell * 0.016);
}

// ---------------------------------------------------------------------------
// Board geometry (treatment-dependent frame/gutter maths — NOT overlay data).
// ---------------------------------------------------------------------------

export type BoardTreatment = "walnut" | "instrument";

/** Snap a pixel size to a multiple of 8 to avoid seam rounding. */
export function snapBoardSize(px: number): number {
  return Math.max(8, Math.round(px / 8) * 8);
}

export interface BoardGeometry {
  /** Grid edge in px (snapped to a multiple of 8). */
  size: number;
  cell: number;
  /** Walnut: round(size × 0.028); instrument: 0. */
  framePad: number;
  /** Coordinate gutter: walnut round(size × 0.052); instrument round(size × 0.038). */
  gutter: number;
  /** Coordinate font size: max(9, round(size × 0.0225)). */
  coordFontSize: number;
  /** File labels sit round(gutter × 0.28) above the frame's bottom edge. */
  coordInsetBottom: number;
  /** Rank labels sit round(gutter × 0.32) in from the frame's left edge. */
  coordInsetLeft: number;
}

/** Frame/gutter geometry as a function of size, per treatment. */
export function boardGeometry(sizePx: number, treatment: BoardTreatment): BoardGeometry {
  const size = snapBoardSize(sizePx);
  const framePad = treatment === "walnut" ? Math.round(size * 0.028) : 0;
  const gutter = Math.round(size * (treatment === "walnut" ? 0.052 : 0.038));
  return {
    size,
    cell: size / 8,
    framePad,
    gutter,
    coordFontSize: Math.max(9, Math.round(size * 0.0225)),
    coordInsetBottom: Math.round(gutter * 0.28),
    coordInsetLeft: Math.round(gutter * 0.32),
  };
}
