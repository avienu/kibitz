/**
 * Tactics screen view-model (design/handoff-2 §Screen: Tactics). Pure
 * logic for the mode column, the seed contract ("Train this weakness"),
 * the reasoning aside and the keyboard map — no DOM, no Tauri.
 */

import type { PlayerProfile } from "./db";
import { parseClaim, shortMotif } from "./profileView";
import type { DrillMode, MotifWeight, ServedPuzzle, TacticsState } from "./tactics";
import { motifWeightsFromProfile } from "./tactics";

/* ---- modes ---------------------------------------------------------------- */

export interface ModeDef {
  id: DrillMode;
  name: string;
  /** Serif one-liner under the name. */
  note: string;
}

/** The five selectable blocks, in design order (weakness is the default). */
export const MODE_DEFS: ModeDef[] = [
  { id: "weakness", name: "Weakness-targeted", note: "Seeded from your motif matrix." },
  { id: "rated", name: "Rated drill", note: "Rating in, rating out." },
  { id: "motif", name: "Motif filter", note: "Pick a motif and grind it." },
  { id: "speed", name: "Heisman speed drill", note: "Easy positions, hard clock." },
  { id: "woodpecker", name: "Woodpecker cycles", note: "The same set, faster each pass." },
];

/** Mono badge per mode — real numbers or nothing, never invented. */
export function modeBadge(
  id: DrillMode,
  st: TacticsState | null,
  profile: PlayerProfile | null,
  woodpeckerSets: number,
): string {
  switch (id) {
    case "weakness": {
      const n = profile?.motifs.filter((m) => m.missed + m.allowed > 0).length ?? 0;
      return n > 0 ? `${n} motifs` : "";
    }
    case "rated":
      return st && st.attempts > 0 ? st.rating.toFixed(0) : "";
    case "motif":
      return st && st.themes.length > 0 ? `${st.themes.length}` : "";
    case "speed":
      return "";
    case "woodpecker":
      return woodpeckerSets > 0 ? `${woodpeckerSets} set${woodpeckerSets === 1 ? "" : "s"}` : "";
  }
}

/** The mono clock renders ONLY in the timed modes. */
export function isTimedMode(mode: DrillMode): boolean {
  return mode === "speed" || mode === "woodpecker";
}

/* ---- seed contract -------------------------------------------------------- */

/** Parse a "Train this weakness" claim into the motif kind it seeds. */
export function seedMotifFromClaim(claim: string | null | undefined): string | null {
  const c = parseClaim(claim ?? null);
  return c && c.kind === "motif" ? c.motif : null;
}

/**
 * Weights for a seeded weakness serve: the weakness selector's weights
 * array IS its motif hint, so restricting it to the seeded kind makes the
 * queue emphasize exactly that motif. Falls back to a synthetic unit
 * weight when no profile (or no row) exists — the mapped themes still
 * get the boost.
 */
export function seededWeights(profile: PlayerProfile | null, motif: string): MotifWeight[] {
  const row = profile?.motifs.find((m) => m.kind === motif);
  if (row && row.missed + row.allowed > 0) {
    return [{ kind: row.kind, allowed: row.allowed, missed: row.missed }];
  }
  return [{ kind: motif, allowed: 1, missed: 0 }];
}

/** Weights for an unseeded weakness serve (the whole profile). */
export function weaknessWeights(
  profile: PlayerProfile | null,
  seededMotif: string | null,
): MotifWeight[] | undefined {
  if (seededMotif) return seededWeights(profile, seededMotif);
  return profile ? motifWeightsFromProfile(profile) : undefined;
}

/* ---- reasoning aside ------------------------------------------------------ */

export type Voice = "coach" | "neutral";

export interface WhyText {
  headline: string;
  body: string;
}

/** Serif headline + body for the WHY THIS PUZZLE aside, voice-aware. The
 * body reuses the backend's per-pick reason verbatim when one exists. */
export function whyText(
  voice: Voice,
  mode: DrillMode,
  served: ServedPuzzle | null,
  extra: { theme?: string; cycleNo?: number; setName?: string },
): WhyText {
  if (!served) {
    return {
      headline: voice === "coach" ? "Nothing on the board yet." : "No puzzle served.",
      body:
        voice === "coach"
          ? "Pick a mode on the left and press Next — the queue explains each pick here."
          : "Select a drill mode and request the next puzzle.",
    };
  }
  const rating = served.puzzle.rating;
  switch (mode) {
    case "weakness": {
      const motif = served.motif ? shortMotif(served.motif) : null;
      const headline = motif
        ? voice === "coach"
          ? `Your games keep paying for ${motif.toLowerCase()} positions — this one drills exactly that.`
          : `${motif}: allowed ${served.allowed}×, missed ${served.missed}× in your profiled games.`
        : voice === "coach"
          ? "No profiled motif matched this one — it keeps the queue honest at your level."
          : "Unmapped themes: served at base weight.";
      const body =
        served.reason ??
        (voice === "coach"
          ? "The selector found nothing profile-specific to say about this pick."
          : "No per-pick reason was recorded.");
      return { headline, body };
    }
    case "rated":
      return {
        headline: voice === "coach" ? "A fair fight at your number." : `Rated ${rating}.`,
        body:
          voice === "coach"
            ? `Rated ${rating}, drawn from within ±100 of your rating. Solve it and your rating moves; fail and it moves the other way.`
            : `Selection band: your rating ±100 (widened only when the band is empty). Result feeds the rating.`,
      };
    case "motif":
      return {
        headline:
          voice === "coach"
            ? `Grinding ${extra.theme ?? "the chosen motif"}.`
            : `Theme filter: ${extra.theme ?? "—"}.`,
        body:
          voice === "coach"
            ? `Every puzzle in this drill carries the ${extra.theme ?? "chosen"} theme — repetition until the pattern is boring.`
            : `Puzzles filtered to the "${extra.theme ?? "—"}" theme near your rating.`,
      };
    case "speed":
      return {
        headline: voice === "coach" ? "Easy position, hard clock." : "Speed drill: sub-rating band.",
        body:
          voice === "coach"
            ? "Deliberately below your rating — the drill trains recognition speed, not depth. The clock is the opponent."
            : "Easy-band selection; the timer is the training variable (Heisman drill).",
      };
    case "woodpecker":
      return {
        headline:
          voice === "coach"
            ? `Cycle ${extra.cycleNo ?? "?"} of "${extra.setName ?? "your set"}".`
            : `Woodpecker cycle ${extra.cycleNo ?? "?"} · ${extra.setName ?? "—"}.`,
        body:
          voice === "coach"
            ? "Same puzzles as last cycle, in the same order — the goal is the same answers, faster."
            : "Fixed set, repeated cycles; compare total time and accuracy across cycles.",
      };
  }
}

/** SOURCE line of the facts block — honest per mode. */
export function sourceFact(mode: DrillMode, served: ServedPuzzle | null): string {
  if (!served) return "—";
  if (mode === "weakness" && served.motif) {
    return `Your profile · allowed ${served.allowed}× · missed ${served.missed}×`;
  }
  return `Lichess puzzle #${served.puzzle.lichessId} (CC0)`;
}

/** MOTIF line: profiled motif in weakness mode; the puzzle's themes after
 * it is finished; a non-spoiling placeholder while solving otherwise. */
export function motifFact(mode: DrillMode, served: ServedPuzzle | null, finished: boolean): string {
  if (!served) return "—";
  if (mode === "weakness" && served.motif) return shortMotif(served.motif);
  if (finished) return served.puzzle.themes.slice(0, 3).join(", ") || "—";
  if (mode === "motif") return "your chosen filter";
  return "revealed when solved";
}

/* ---- keyboard ------------------------------------------------------------- */

export type TacticsKey = "hint" | "skip" | "giveup" | "next";

/** Map a keydown to an action; editable targets swallow everything. */
export function tacticsKeyAction(key: string, editable: boolean): TacticsKey | null {
  if (editable) return null;
  switch (key) {
    case "h":
    case "H":
      return "hint";
    case "s":
    case "S":
      return "skip";
    case "g":
    case "G":
      return "giveup";
    case "Enter":
      return "next";
    default:
      return null;
  }
}
