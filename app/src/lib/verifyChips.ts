/**
 * CONSIDER-chip verification state machine (run 11). Pure — no Tauri, no
 * DOM — so the pending→cleared/refuted transitions and the stale-drop
 * rule are unit-testable.
 *
 * The static explanation renders instantly; when its WSUI screen fired
 * and suggestions exist, App fires one `verify_suggestions` round-trip
 * (the maintainer-sanctioned engine trigger). Chip rules:
 *  - statically-clean chips show immediately, with a subtle pending
 *    affordance only while verification is running; a refuted chip
 *    disappears;
 *  - statically-marked chips (static_risk) are NOT rendered until the
 *    engine clears them; if the engine is unavailable they simply stay
 *    hidden;
 *  - results are FEN-stamped (the engine-info pattern, audit 2026-07
 *    #5): a result whose stamp does not match the request is dropped.
 */

import type { ExplanationJson, SuggestionJson } from "./gameView";

export type ChipVerdict = "cleared" | "refuted";

/** `verify_suggestions` response (camelCase over the wire). */
export interface VerifyOut {
  fen: string;
  /** False = the server-side gate declined (quiet / nothing to verify):
   * no engine ran and no verdicts exist. */
  ran: boolean;
  verdicts: { uci: string; san: string; verdict: ChipVerdict }[];
  nodesPerSearch: number;
}

export type VerificationState =
  /** Round-trip in flight for `fen`. */
  | { kind: "running"; fen: string }
  /** Verdicts landed for `fen`. */
  | { kind: "done"; fen: string; verdicts: Record<string, ChipVerdict> }
  /** Engine unavailable / call failed: marked chips stay hidden, clean
   * chips lose the pending affordance. */
  | { kind: "unavailable" };

/**
 * Should App fire a verification round-trip for this explanation? Only
 * the sanctioned trigger: the tactical screen fired AND chips exist
 * (marked ones count — they are exactly what needs resurrecting). A
 * quiet position never causes an engine round-trip.
 */
export function needsVerification(explanation: ExplanationJson | null): boolean {
  return (
    explanation !== null &&
    explanation.tag === "TACTICAL SCREEN FIRED" &&
    (explanation.suggestions?.length ?? 0) > 0
  );
}

/**
 * Fold a settled round-trip into the state. Stale results — the stamp
 * differs from the FEN we are running for — leave the state untouched.
 */
export function resolveVerification(
  state: VerificationState,
  result: VerifyOut,
): VerificationState {
  if (state.kind !== "running" || state.fen !== result.fen) return state;
  if (!result.ran) return { kind: "unavailable" };
  const verdicts: Record<string, ChipVerdict> = {};
  for (const v of result.verdicts) verdicts[v.uci] = v.verdict;
  return { kind: "done", fen: result.fen, verdicts };
}

/** Fold a failed round-trip (engine unavailable) into the state. */
export function failVerification(
  state: VerificationState,
  requestedFen: string,
): VerificationState {
  if (state.kind !== "running" || state.fen !== requestedFen) return state;
  return { kind: "unavailable" };
}

/** One renderable chip: `index` addresses the ORIGINAL suggestion slot
 * (evidence hover indices are blocks.length + index). */
export interface VisibleChip {
  s: SuggestionJson;
  index: number;
  pending: boolean;
}

/**
 * The chips to render for an explanation under a verification state
 * (null = no verification was ever needed or started — statically clean
 * chips show plain, marked chips stay hidden).
 */
export function visibleChips(
  explanation: ExplanationJson | null,
  verification: VerificationState | null,
): VisibleChip[] {
  const suggestions = explanation?.suggestions ?? [];
  const running = verification?.kind === "running";
  const verdicts = verification?.kind === "done" ? verification.verdicts : null;
  const out: VisibleChip[] = [];
  suggestions.forEach((s, index) => {
    const marked = s.static_risk != null;
    const verdict = verdicts?.[s.uci] ?? null;
    if (verdict === "refuted") return;
    if (marked && verdict !== "cleared") return;
    out.push({ s, index, pending: running && verdict === null });
  });
  return out;
}
