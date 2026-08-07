/**
 * Game-view state + derivations (design/handoff-1/README.md §Screen 2,
 * §Interactions & Behavior, §State Management).
 *
 * Everything here is pure logic — no DOM, no Tauri — so the prose⇄board
 * linkage, keyboard map, eval-bar state derivation and resize snapping are
 * all unit-testable in isolation. App.tsx wires this to React.
 */

import { whitePovCp, type AnalysisRow } from "./analyses";
import {
  DEFAULT_INTENSITY,
  type Evidence,
  type EvidenceArrow,
} from "./evidence";

/* ------------------------------------------------------------------ */
/* The explanation contract (backend: kibitz_core::record::Explanation,
 * serialized snake_case; evidence arrays are omitted when empty).      */
/* ------------------------------------------------------------------ */

export interface VoiceText {
  coach: string;
  neutral: string;
}

export type ExplainBlockKind = "alert" | "imbalance" | "plan" | "scheme";

/**
 * Blocks are grouped by HORIZON, not by kind. A tactic on the board and a
 * five-move regrouping are different sorts of advice, and letting them
 * compete for one list is how the long game gets buried under whatever is
 * urgent (maintainer, run 12: "I don't see LONG TERM plans").
 */
export type Horizon = "now" | "next" | "long";

export const BLOCK_HORIZON: Record<ExplainBlockKind, Horizon> = {
  alert: "now",
  imbalance: "now",
  plan: "next",
  scheme: "long",
};

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

/** One candidate move (run 10): computed statically by kibitz-core::suggest,
 * delivered inside the contract — the UI never synthesizes suggestions. */
export interface SuggestionJson {
  san: string;
  uci: string;
  score: number;
  /** Hint tokens the move serves; denied opponent tokens lead when
   * prophylactic. May be absent (serde skips empty vecs). */
  serving?: string[];
  prophylactic: boolean;
  /** Whole-board static veto mark (run 11): present when the static
   * screen found the move leaves a piece en prise (net SEE swing, cp).
   * Marked chips are NEVER rendered until engine verification clears
   * them (lib/verifyChips.ts). Absent when statically clean. */
  static_risk?: number | null;
  /** The move as a key arrow, for chip-hover isolation. */
  evidence: EvidenceJson;
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
  /** Candidate moves, best first; absent when a confirmed tactic or
   * decisive line gates positional talk. */
  suggestions?: SuggestionJson[];
}

/** Fill the omitted-when-empty arrays so downstream code never branches. */
export function normalizeEvidence(
  e: EvidenceJson | null | undefined,
): Evidence {
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
 * Block indices hidden by the summary-first rule (design round 3 change
 * note, supersedes the run-10 top-3 alert collapse): only the FIRST
 * finding renders until the pinned-foot expander opens the rest. The
 * verbalizer emits blocks most-important first (alerts by severity), so
 * the leading block IS the summary. Empty when expanded.
 */
export function hiddenFindingIndices(
  blocks: ExplanationBlockJson[],
  expanded: boolean,
): number[] {
  if (expanded) return [];
  // Summary mode keeps the LEADING finding of each horizon, not just the
  // first block overall (run 12). Collapsing to one line buried the long
  // game behind whatever was most urgent, which is the exact complaint
  // the horizon split exists to answer — a long-term plan that only
  // appears once you expand is a long-term plan nobody reads.
  const leading = new Set<Horizon>();
  const shown = new Set<number>();
  blocks.forEach((b, i) => {
    const horizon = BLOCK_HORIZON[b.kind];
    if (leading.has(horizon)) return;
    leading.add(horizon);
    shown.add(i);
  });
  return blocks.map((_, i) => i).filter((i) => !shown.has(i));
}

/** Options for [`deriveEvidence`]. */
export interface EvidenceOptions {
  /** A variation preview is active: the explanation is PAUSED on the main
   * game, so its rings/arrows must never paint over the previewed
   * position (audit 2026-07 #4). */
  previewing?: boolean;
}

/**
 * Sentinel hover index for the moves panel's COACH narration rows
 * (run 10 unification): it addresses neither a block nor a suggestion,
 * so [`deriveEvidence`] falls through to the visible union while
 * [`deriveIntensity`] goes to full — "light up everything this ply's
 * explanation shows", through the ONE existing evidence path. -1 can
 * never collide with a real block/suggestion index.
 */
export const COACH_HOVER_INDEX = -1;

/**
 * README §State Management: evidence = hovered sentence's evidence alone,
 * else the union of all blocks; null when there is no explanation.
 * Indices past the block list address the suggestion chips (run 10):
 * index blocks.length + j isolates suggestion j's key arrow. Suggestion
 * evidence is hover-only — it never joins the no-hover union.
 * [`COACH_HOVER_INDEX`] (or any index addressing nothing) keeps the
 * union but at hover intensity — the COACH-row hover contract.
 * While a variation preview is active, NO evidence renders at all — the
 * paused main-game overlays would point at squares whose pieces moved.
 */
export function deriveEvidence(
  explanation: ExplanationJson | null,
  hoverSentence: number | null,
  opts: EvidenceOptions = {},
): Evidence | null {
  if (!explanation || opts.previewing) return null;
  if (hoverSentence !== null) {
    const block = explanation.blocks[hoverSentence];
    if (block) return normalizeEvidence(block.evidence);
    const suggestion =
      explanation.suggestions?.[hoverSentence - explanation.blocks.length];
    if (suggestion) return normalizeEvidence(suggestion.evidence);
  }
  // The no-hover union ALWAYS covers every finding (design round 3
  // change note: collapsing hides prose, never evidence — the expander
  // even says "evidence is already on the board").
  return unionEvidence(explanation.blocks);
}

/** "PressureBackwardPawn" -> "pressure backward pawn", for chip tooltips. */
export function humanizeHintToken(token: string): string {
  return token.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
}

/** Tooltip text for a suggestion chip: served plans, denial flagged. */
export function suggestionTitle(s: SuggestionJson): string {
  const serving = (s.serving ?? []).map(humanizeHintToken).join(", ");
  const base = serving.length > 0 ? `serves: ${serving}` : "candidate move";
  return s.prophylactic ? `denies the opponent's plan — ${base}` : base;
}

/** Intensity: 1.0 while a sentence is hovered, else the 0.44 baseline. */
export function deriveIntensity(hoverSentence: number | null): number {
  return hoverSentence !== null ? 1 : DEFAULT_INTENSITY;
}

/** Does this block's evidence reference `square` (marks or arrows)? */
export function blockReferencesSquare(
  block: ExplanationBlockJson,
  square: string,
): boolean {
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
  /** Restore a persisted orientation (e.g. resuming from Home). */
  | { type: "setFlipped"; flipped: boolean }
  /** A new game was installed: jump to `ply`, drop transient state. */
  | { type: "gameLoaded"; ply: number; plyCount: number };

const clamp = (n: number, lo: number, hi: number) =>
  Math.max(lo, Math.min(hi, n));

export function reduceGameView(
  state: GameViewState,
  action: GameViewAction,
): GameViewState {
  switch (action.type) {
    case "setFlipped":
      return { ...state, flipped: action.flipped };
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
        selectedSquare:
          state.selectedSquare === action.square ? null : action.square,
      };
    case "clearSelection":
      return state.selectedSquare === null
        ? state
        : { ...state, selectedSquare: null };
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

/**
 * Ply to open a RESUMED game at (audit 2026-07 #12). Finished games are
 * annotated to the end, so "last touched" equals the final ply for every
 * reviewed game — resuming there shows only the last position. When the
 * saved ply is the game's final ply (or past it), open at the start
 * instead; any genuinely mid-game bookmark is honored as-is.
 */
export function chooseResumePly(savedPly: number, plyCount: number): number {
  if (plyCount > 0 && savedPly >= plyCount) return 0;
  return Math.max(0, savedPly);
}

/**
 * Which way to face the board when opening a game (2026-08-02 field
 * report: "if I'm one of the players it should always start with my side
 * towards me"). True = Black at the bottom.
 *
 * `mine` is every name form the user's identity resolves to — chess.com
 * and Lichess handles included, since a game's players are recorded under
 * whatever handle played it, not under the canonical name. Matching is
 * case-insensitive and trimmed for the same reason.
 *
 * Playing BOTH sides (an engine game against yourself, or a name collision)
 * is not a preference — it falls back to White, as does a game the user is
 * not in.
 */
export function openingOrientation(
  white: string,
  black: string,
  mine: readonly string[],
): boolean {
  const norm = (s: string) => s.trim().toLowerCase();
  const set = new Set(mine.map(norm));
  const isWhite = set.has(norm(white));
  const isBlack = set.has(norm(black));
  return isBlack && !isWhite;
}

/* ------------------------------------------------------------------ */
/* Keyboard map (README §Interactions & Behavior)                      */
/* ------------------------------------------------------------------ */

export type GameKeyAction =
  "next" | "prev" | "fwd5" | "back5" | "start" | "end" | "flip" | "explain";

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

export function isEditableTarget(
  t: EditableTargetLike | null | undefined,
): boolean {
  if (!t) return false;
  const tag = (t.tagName ?? "").toUpperCase();
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    t.isContentEditable === true
  );
}

/**
 * ←/→ ±1 ply, ↓/↑ ±5, Home/End |◀/▶|, f flip, e explain. Global to the
 * game view — but never while a text input is focused, and never with a
 * modifier held.
 */
export function keyboardAction(
  key: string,
  opts: KeyboardOpts = {},
): GameKeyAction | null {
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
 * (app/kibitz-db/src/engine.rs); anything at or beyond this threshold is
 * displayed as a mate, never as a centipawn number.
 */
export const MATE_SENTINEL_CP = 9500;

/** Pick the row the eval bar shows for a ply: fresh beats legacy; within
 * a kind the first row wins (`game_analyses` orders newest first). */
export function selectPlyAnalysis(
  rows: AnalysisRow[],
  ply: number,
): AnalysisRow | null {
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
export function evalBarView(
  row: AnalysisRow | null,
  mate?: number | null,
): EvalBarView {
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
  if (!row)
    return {
      state: "no-data",
      fillPct: null,
      readout: "—",
      tooltip: "no analysis",
    };
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
  while (
    size > MIN_BOARD_SIZE &&
    size + boardOverhead(size, treatment) > avail
  ) {
    size -= 8;
  }
  return Math.max(MIN_BOARD_SIZE, size);
}
