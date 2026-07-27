/**
 * First-run tour (design/handoff-2 §Help & first-run tour): one card per
 * rail group, anchored BESIDE the group it describes, never covering it.
 * Pure state machine + step data — no DOM — unit-testable; the overlay
 * component does the measuring and rendering.
 */

/** Which rail element a card anchors beside. */
export type TourAnchor = "header" | "study" | "coach" | "train" | "data" | "footer";

export interface TourStep {
  id: string;
  anchor: TourAnchor;
  /** Bolded lead-in ("This is Coach."). */
  title: string;
  /** Serif body prose. */
  body: string;
}

/** One card per rail group (header · four groups · footer = 6). */
export const TOUR_STEPS: TourStep[] = [
  {
    id: "rail",
    anchor: "header",
    title: "This is the rail.",
    body:
      "It is the whole app's router — every capability has a home here, " +
      "including the command-line-only ones. The badges are live data or " +
      "absent, never fake numbers.",
  },
  {
    id: "study",
    anchor: "study",
    title: "Study.",
    body:
      "Your games live here: the Database, the Game view (the centrepiece " +
      "— board, Explain, moves), the transposition-aware Opening tree, and " +
      "Position search.",
  },
  {
    id: "coach",
    anchor: "coach",
    title: "This is Coach.",
    body:
      "Everything Silman knows about your play lives in these screens — " +
      "Explain reads positions, Profile reads your games, Prep reads your " +
      "opponent. No engine runs unless a screen fires or you ask.",
  },
  {
    id: "train",
    anchor: "train",
    title: "Train.",
    body:
      "The daily queues: Openings SRS (FSRS-scheduled repertoire cards), " +
      "Tactics, and the tiered Endgames curriculum graded against " +
      "tablebase truth.",
  },
  {
    id: "data",
    anchor: "data",
    title: "Data in / out.",
    body:
      "Imports (PGN, SCID), the TWIC ingester, account syncs and the Jobs " +
      "queue — the only place the engine actually runs.",
  },
  {
    id: "footer",
    anchor: "footer",
    title: "Settings and Help.",
    body:
      "Settings names the engine spawn policy in words; Help & tour holds " +
      "the full user guide — and can replay this tour any time.",
  },
];

export interface TourState {
  step: number;
  done: boolean;
}

export type TourAction = { type: "next" } | { type: "skip" } | { type: "replay" };

export function initialTour(): TourState {
  return { step: 0, done: false };
}

/** next advances (finishing past the last card); skip ends immediately;
 * replay restarts from the first card. */
export function reduceTour(
  s: TourState,
  a: TourAction,
  total: number = TOUR_STEPS.length,
): TourState {
  switch (a.type) {
    case "next":
      return s.step + 1 >= total ? { ...s, done: true } : { step: s.step + 1, done: false };
    case "skip":
      return { ...s, done: true };
    case "replay":
      return { step: 0, done: false };
  }
}

/** The card's mono counter, e.g. "2 / 6". */
export function tourCounter(step: number, total: number = TOUR_STEPS.length): string {
  return `${step + 1} / ${total}`;
}
