/**
 * Game-view state + derivations (design/handoff-1/README.md §Screen 2,
 * §Interactions & Behavior, §State Management).
 *
 * Everything here is pure logic — no DOM, no Tauri — so the prose⇄board
 * linkage, keyboard map, eval-bar state derivation and resize snapping are
 * all unit-testable in isolation. App.tsx wires this to React.
 */

import { whitePovCp, type AnalysisRow } from "./analyses";
import { DEFAULT_INTENSITY, type Evidence, type EvidenceArrow } from "./evidence";

/* ------------------------------------------------------------------ */
/* The explanation contract (backend: silman_core::record::Explanation,
 * serialized snake_case; evidence arrays are omitted when empty).      */
/* ------------------------------------------------------------------ */

export interface VoiceText {
  coach: string;
  neutral: string;
}

export type ExplainBlockKind = "alert" | "imbalance" | "plan";

/** Wire evidence: every array may be absent (serde skips empty vecs). */
export interface EvidenceJson {
  alerts?: string[];
  attackers?: string[];
  defenders?: string[];
  imbalance?: string[];
  key?: string[];
  arrows?: EvidenceArrow[];
}

export interface ExplanationBlockJson {
  kind: ExplainBlockKind;
  text: VoiceText;
  evidence: EvidenceJson;
}

export interface EvalReadoutJson {
  cp?: number | null;
  mate?: number | null;
  /** Preformatted: "+2.6" or "#5" (negative mate = Black mates). */
  display: string;
}

/** The per-ply explanation object (schema v3). The UI never synthesizes
 * explanations — this arrives whole from `explain_position`. */
export interface ExplanationJson {
  schema_version: number;
  /** "TACTICAL SCREEN FIRED" | "FORCED MATE" | "QUIET POSITION". */
  tag: string;
  eval?: EvalReadoutJson | null;
  headline: VoiceText;
  blocks: ExplanationBlockJson[];
}

/** Fill the omitted-when-empty arrays so downstream code never branches. */
export function normalizeEvidence(e: EvidenceJson | null | undefined): Evidence {
  return {
    alerts: e?.alerts ?? [],
    attackers: e?.attackers ?? [],
    defenders: e?.defenders ?? [],
    imbalance: e?.imbalance ?? [],
    key: e?.key ?? [],
    arrows: e?.arrows ?? [],
  };
}

/** Union of every block's evidence (the no-hover default overlay set). */
export function unionEvidence(blocks: ExplanationBlockJson[]): Evidence {
  const out = normalizeEvidence(null);
  for (const b of blocks) {
    const e = normalizeEvidence(b.evidence);
    out.alerts.push(...e.alerts);
    out.attackers.push(...e.attackers);
    out.defenders.push(...e.defenders);
    out.imbalance.push(...e.imbalance);
    out.key.push(...e.key);
    out.arrows.push(...e.arrows);
  }
  return out;
}

/**
 * README §State Management: evidence = hovered sentence's evidence alone,
 * else the union of all blocks; null when there is no explanation.
 */
export function deriveEvidence(
  explanation: ExplanationJson | null,
  hoverSentence: number | null,
): Evidence | null {
  if (!explanation) return null;
  if (hoverSentence !== null) {
    const block = explanation.blocks[hoverSentence];
    if (block) return normalizeEvidence(block.evidence);
  }
  return unionEvidence(explanation.blocks);
}

/** Intensity: 1.0 while a sentence is hovered, else the 0.44 baseline. */
export function deriveIntensity(hoverSentence: number | null): number {
  return hoverSentence !== null ? 1 : DEFAULT_INTENSITY;
}

/** Does this block's evidence reference `square` (marks or arrows)? */
export function blockReferencesSquare(block: ExplanationBlockJson, square: string): boolean {
  const e = normalizeEvidence(block.evidence);
  return (
    e.alerts.includes(square) ||
    e.attackers.includes(square) ||
    e.defenders.includes(square) ||
    e.imbalance.includes(square) ||
    e.key.includes(square) ||
    e.arrows.some((a) => a.from === square || a.to === square)
  );
}

/**
 * Prose filtering (README §Interactions): with a square selected,
 * sentences that don't reference it drop to opacity 0.34.
 */
export function sentenceOpacity(
  block: ExplanationBlockJson,
  selectedSquare: string | null,
): number {
  if (selectedSquare === null) return 1;
  return blockReferencesSquare(block, selectedSquare) ? 1 : 0.34;
}

/** Footer meta selection state text. */
export function selectionNote(selectedSquare: string | null): string {
  return selectedSquare === null
    ? "hover a line to isolate its evidence"
    : `filtered to ${selectedSquare}`;
}

/* ------------------------------------------------------------------ */
/* GameViewState + reducer (README §State Management)                  */
/* ------------------------------------------------------------------ */

export type Voice = "coach" | "neutral";
export type AnnotationMode = "full" | "hover" | "hidden";
export type BoardTreatmentChoice = "walnut" | "instrument";
export type Theme = "dark" | "light";

export interface GameViewState {
  /** 0..plyCount, index into the position list. */
  ply: number;
  /** Index into the current explanation's blocks. */
  hoverSentence: number | null;
  /** e.g. "d7". */
  selectedSquare: string | null;
  voice: Voice;
  annotationMode: AnnotationMode;
  boardTreatment: BoardTreatmentChoice;
  theme: Theme;
  flipped: boolean;
}

export type GameViewAction =
  | { type: "setPly"; ply: number; plyCount: number }
  | { type: "step"; delta: number; plyCount: number }
  | { type: "hoverSentence"; index: number | null }
  /** Toggles: clicking the selected square again clears the selection. */
  | { type: "selectSquare"; square: string }
  | { type: "clearSelection" }
  | { type: "setVoice"; voice: Voice }
  | { type: "setAnnotationMode"; mode: AnnotationMode }
  | { type: "setTreatment"; treatment: BoardTreatmentChoice }
  | { type: "setTheme"; theme: Theme }
  | { type: "toggleFlip" }
  /** A new game was installed: jump to `ply`, drop transient state. */
  | { type: "gameLoaded"; ply: number; plyCount: number };

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

export function reduceGameView(state: GameViewState, action: GameViewAction): GameViewState {
  switch (action.type) {
    case "setPly":
    case "gameLoaded": {
      const ply = clamp(action.ply, 0, action.plyCount);
      // Stepping a move clears both hover and selection (README).
      return { ...state, ply, hoverSentence: null, selectedSquare: null };
    }
    case "step": {
      const ply = clamp(state.ply + action.delta, 0, action.plyCount);
      return { ...state, ply, hoverSentence: null, selectedSquare: null };
    }
    case "hoverSentence":
      return state.hoverSentence === action.index
        ? state
        : { ...state, hoverSentence: action.index };
    case "selectSquare":
      return {
        ...state,
        selectedSquare: state.selectedSquare === action.square ? null : action.square,
      };
    case "clearSelection":
      return state.selectedSquare === null ? state : { ...state, selectedSquare: null };
    case "setVoice":
      return { ...state, voice: action.voice };
    case "setAnnotationMode":
      return { ...state, annotationMode: action.mode };
    case "setTreatment":
      return { ...state, boardTreatment: action.treatment };
    case "setTheme":
      return { ...state, theme: action.theme };
    case "toggleFlip":
      return { ...state, flipped: !state.flipped };
  }
}

/* ------------------------------------------------------------------ */
/* Keyboard map (README §Interactions & Behavior)                      */
/* ------------------------------------------------------------------ */

export type GameKeyAction =
  | "next"
  | "prev"
  | "fwd5"
  | "back5"
  | "start"
  | "end"
  | "flip"
  | "explain";

export interface KeyboardOpts {
  /** True when a text input / textarea / contenteditable has focus. */
  editable?: boolean;
  /** True when a modifier (meta/ctrl/alt) is held — leave those alone. */
  modifier?: boolean;
}

/** Minimal shape of an event target for editable detection (testable). */
export interface EditableTargetLike {
  tagName?: string;
  isContentEditable?: boolean;
}

export function isEditableTarget(t: EditableTargetLike | null | undefined): boolean {
  if (!t) return false;
  const tag = (t.tagName ?? "").toUpperCase();
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || t.isContentEditable === true;
}

/**
 * ←/→ ±1 ply, ↓/↑ ±5, Home/End |◀/▶|, f flip, e explain. Global to the
 * game view — but never while a text input is focused, and never with a
 * modifier held.
 */
export function keyboardAction(key: string, opts: KeyboardOpts = {}): GameKeyAction | null {
  if (opts.editable || opts.modifier) return null;
  switch (key) {
    case "ArrowRight":
      return "next";
    case "ArrowLeft":
      return "prev";
    case "ArrowDown":
      return "fwd5";
    case "ArrowUp":
      return "back5";
    case "Home":
      return "start";
    case "End":
      return "end";
    case "f":
    case "F":
      return "flip";
    case "e":
    case "E":
      return "explain";
    default:
      return null;
  }
}

/* ------------------------------------------------------------------ */
/* Eval bar (deliverable 2a)                                           */
/* ------------------------------------------------------------------ */

/**
 * Fresh analyses store mate as an eval_cp sentinel of ±10000
 * (app/silman-db/src/engine.rs); anything at or beyond this threshold is
 * displayed as a mate, never as a centipawn number.
 */
export const MATE_SENTINEL_CP = 9500;

/** Pick the row the eval bar shows for a ply: fresh beats legacy; within
 * a kind the first row wins (`game_analyses` orders newest first). */
export function selectPlyAnalysis(rows: AnalysisRow[], ply: number): AnalysisRow | null {
  let legacy: AnalysisRow | null = null;
  for (const r of rows) {
    if (r.ply !== ply) continue;
    if (r.kind === "fresh") return r;
    if (!legacy) legacy = r;
  }
  return legacy;
}

export type EvalBarView =
  | { state: "no-data"; fillPct: null; readout: "—"; tooltip: "no analysis" }
  | { state: "cp"; fillPct: number; readout: string; tooltip: string }
  | {
      state: "mate";
      /** Pinned to the winning side: 94 for White, 6 for Black. */
      fillPct: 94 | 6;
      /** "#N", or "#" when the stored sentinel has no mate distance. */
      readout: string;
      /** Readout renders in the winning side's colour:
       *  white → var(--accent), black → var(--bad). */
      winner: "white" | "black";
      tooltip: string;
    };

/** Source line for the eval tooltip. */
export function evalSourceLabel(row: AnalysisRow): string {
  if (row.kind === "fresh") {
    const detail =
      row.depth !== null
        ? ` · depth ${row.depth}`
        : row.nodes !== null
          ? ` · nodes ${row.nodes.toLocaleString("en-US")}`
          : "";
    return `${row.engine}${detail} (fresh)`;
  }
  return `legacy import · ${row.engine}`;
}

/**
 * Eval-bar state for one ply. White's share of the track is
 * clamp(6%, 50 + pawns×9, 94%); no stored analysis renders the muted
 * NO-DATA state (never a fake 0.0); the mate sentinel pins the bar.
 * `mate` (signed, White POV; from an explanation readout when known)
 * supplies the mate distance the analyses table can't store.
 */
export function evalBarView(row: AnalysisRow | null, mate?: number | null): EvalBarView {
  if (mate != null && mate !== 0) {
    const winner = mate > 0 ? "white" : "black";
    return {
      state: "mate",
      fillPct: winner === "white" ? 94 : 6,
      readout: `#${Math.abs(mate)}`,
      winner,
      tooltip: row ? evalSourceLabel(row) : "no analysis",
    };
  }
  if (!row) return { state: "no-data", fillPct: null, readout: "—", tooltip: "no analysis" };
  const cp = whitePovCp(row.kind, row.ply, row.evalCp);
  if (Math.abs(cp) >= MATE_SENTINEL_CP) {
    const winner = cp > 0 ? "white" : "black";
    return {
      state: "mate",
      fillPct: winner === "white" ? 94 : 6,
      readout: "#",
      winner,
      tooltip: evalSourceLabel(row),
    };
  }
  const pawns = cp / 100;
  const fillPct = clamp(50 + pawns * 9, 6, 94);
  const s = pawns.toFixed(1);
  return {
    state: "cp",
    fillPct,
    readout: pawns >= 0 ? `+${s}` : s,
    tooltip: evalSourceLabel(row),
  };
}

/* ------------------------------------------------------------------ */
/* Resize (deliverable 2c)                                             */
/* ------------------------------------------------------------------ */

/** Documented minimum window size for the shell (tauri.conf.json). */
export const MIN_WINDOW = { width: 1180, height: 760 } as const;

/** Board grid never goes below this (multiple of 8). */
export const MIN_BOARD_SIZE = 496;

/** The nav rail collapses to icon-only 56px below this window width. */
export const RAIL_COLLAPSE_WIDTH = 1280;

export function railCollapsed(windowWidth: number): boolean {
  return windowWidth < RAIL_COLLAPSE_WIDTH;
}

/** Per-treatment chrome around the grid (matches lib/evidence.ts
 * boardGeometry): walnut framePad×2 + gutter, instrument gutter only. */
function boardOverhead(size: number, treatment: BoardTreatmentChoice): number {
  const framePad = treatment === "walnut" ? Math.round(size * 0.028) : 0;
  const gutter = Math.round(size * (treatment === "walnut" ? 0.052 : 0.038));
  return framePad * 2 + gutter;
}

/**
 * Largest multiple-of-8 grid size whose full board (grid + frame +
 * coordinate gutter) fits in the available box; never below
 * MIN_BOARD_SIZE (at that point the column scrolls/clips rather than
 * shrinking the board further).
 */
export function fitBoardSize(
  availW: number,
  availH: number,
  treatment: BoardTreatmentChoice = "walnut",
): number {
  const avail = Math.min(availW, availH);
  let size = Math.floor(avail / 8) * 8;
  while (size > MIN_BOARD_SIZE && size + boardOverhead(size, treatment) > avail) {
    size -= 8;
  }
  return Math.max(MIN_BOARD_SIZE, size);
}
