/**
 * Opponent-prep workflow view-model (design/handoff-2 §Screen: Opponent
 * prep). Pure logic for the four-step stepper, the fingerprint table, the
 * weak-line prose and the aside's profile finding — no DOM, no Tauri.
 */

import type { BookExit, FingerprintRow, PlayerProfile, PrepEntry, WeakLine } from "./db";
import { humanMotif, rankedMotifs } from "./profileView";

export type PrepColor = "white" | "black";

/* ---- stepper -------------------------------------------------------------- */

export interface PrepStepValues {
  opponent: string | null;
  color: PrepColor;
  /** Name of the selected weak line (once one is chosen). */
  lineName: string | null;
  /** Master-game count of the selected line (once reached). */
  masterCount: number | null;
}

/** The four chips with their chosen values (shown once passed/reached). */
export function stepperSteps(v: PrepStepValues, reached: number) {
  return [
    { label: "Opponent", value: v.opponent },
    { label: "Fingerprint", value: v.opponent && reached >= 2 ? (v.color === "white" ? "as White" : "as Black") : null },
    { label: "Weak lines", value: reached >= 3 ? v.lineName : null },
    {
      label: "Master games",
      value: reached >= 4 && v.masterCount !== null ? `${v.masterCount} game${v.masterCount === 1 ? "" : "s"}` : null,
    },
  ];
}

/* ---- weak lines ----------------------------------------------------------- */

/** Display name for a weak line: opening name, else ECO, else honest. */
export function lineName(l: WeakLine): string {
  return l.openingName ?? l.eco ?? "Out of book";
}

/** Mono move summary: what the opponent actually plays at the spot. */
export function lineMoves(l: WeakLine): string {
  const moves = l.opponentMoves.slice(0, 3).join(" · ");
  return `by ply ${l.ply} · plays ${moves}`;
}

/** "38% in 21" — the red mono score chip. */
export function lineScore(l: WeakLine): string {
  return `${l.scorePct.toFixed(0)}% in ${l.games}`;
}

/** Serif why-this-is-weak paragraph, citing real counts only. */
export function lineWhy(opponent: string, color: PrepColor, l: WeakLine): string {
  const name = lineName(l);
  const s: string[] = [];
  s.push(
    `${opponent} reaches this ${l.eco ? `${name} position` : "position"} in ${l.games} games as ${color === "white" ? "White" : "Black"} and scores ${l.scorePct.toFixed(1)}%.`,
  );
  if (l.opponentMoves.length > 0) {
    s.push(
      l.opponentMoves.length === 1
        ? `The reply is automatic: ${l.opponentMoves[0]} every time.`
        : `The usual choice here is ${l.opponentMoves[0]}; ${l.opponentMoves.slice(1).join(" and ")} also appear.`,
    );
  }
  if (l.deviation) {
    s.push("This is also one of the book-exit points — the first move that leaves known theory.");
  }
  return s.join(" ");
}

/** The ranking rule, stated honestly (silman-db prep defaults). */
export const MASTER_RANKING_RULE =
  "Games reaching this exact position with both players rated 2200 or above, strongest pairings first.";

/* ---- fingerprint ---------------------------------------------------------- */

/** Weak-family rule for the --bad bar/score: under 50% with a real sample. */
export function fingerprintRowWeak(r: FingerprintRow): boolean {
  return r.scorePct < 50 && r.games >= 3;
}

/** Book-exit summary for an ECO family row, matched by code; null = none. */
export function bookExitFor(row: FingerprintRow, exits: BookExit[]): string | null {
  const hit = exits.find((e) => e.eco === row.eco);
  if (!hit) return null;
  return `leaves book at ${hit.san} (ply ${hit.ply}, ${hit.count}×)`;
}

/* ---- prep state ----------------------------------------------------------- */

/** "YYYY-MM-DD HH:MM:SS" UTC — matches the backend's now_utc format. */
export function prepTimestamp(now: Date): string {
  return now.toISOString().slice(0, 19).replace("T", " ");
}

/** Upsert a started prep (one entry per opponent+color, newest first). */
export function recordPrep(entries: PrepEntry[], opponent: string, color: PrepColor, now: Date): PrepEntry[] {
  const rest = entries.filter((e) => !(e.opponent === opponent && e.color === color));
  return [{ opponent, color, startedAt: prepTimestamp(now) }, ...rest];
}

/* ---- the aside's profile finding ------------------------------------------ */

/**
 * A finding about THIS opponent from their built profile, or an honest
 * absence line. `profile` must belong to the opponent (caller checks the
 * name) — never substitute someone else's findings.
 */
export function prepFinding(opponent: string | null, profile: PlayerProfile | null): string {
  if (!opponent) {
    return "Pick an opponent to see what their profile says.";
  }
  if (!profile || profile.player !== opponent) {
    return `No profile has been built for ${opponent} yet — open their profile to build one; the findings will surface here.`;
  }
  const top = rankedMotifs(profile)[0];
  if (top) {
    const bits: string[] = [];
    if (top.allowed > 0) bits.push(`allows it against themselves ${top.allowed}×`);
    if (top.missed > 0) bits.push(`missed ${top.missed} of ${top.opportunities} chances to use it`);
    return `Their profile's loudest motif is ${humanMotif(top.kind)}: ${opponent} ${bits.join(" and ")} across ${profile.games} profiled games.`;
  }
  const weak = profile.structures
    .filter((s) => s.games >= 2)
    .slice()
    .sort((a, b) => a.score_pct - b.score_pct)[0];
  if (weak && weak.score_pct < 50) {
    return `Their profile's weakest spot is structural: ${weak.score_pct.toFixed(0)}% over ${weak.games} games in ${weak.flag.replace(/-/g, " ")} positions.`;
  }
  return `${opponent}'s profile (${profile.games} games) shows no dominant motif or structural weakness — lean on the line ranking instead.`;
}
