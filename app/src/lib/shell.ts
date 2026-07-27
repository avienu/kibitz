/**
 * Shell view model (design/handoff-1/README.md §A Nav rail): the rail's
 * groups/items and the badge formatting rules. Badges show real data or
 * nothing — never fake numbers.
 *
 * Pure logic — no DOM, no Tauri — unit-testable in isolation.
 */

import type { JobsStatus, PlayerProfile, TrainSummary } from "./db";
import type { TacticsState } from "./tactics";

/** Every routable surface in the app (the discoverability fix). */
export type ViewId =
  | "home"
  | "database"
  | "game"
  | "tree"
  | "search"
  | "profile"
  | "prep"
  | "train"
  | "tactics"
  | "endgames"
  | "import"
  | "twic"
  | "syncs"
  | "jobs"
  | "settings";

export interface RailItem {
  id: ViewId | "explain" | "help";
  label: string;
  /** Icon-only collapsed-rail glyph (two chars max). */
  icon: string;
}

export interface RailGroup {
  heading: string;
  items: RailItem[];
}

/** The rail structure is fixed by the design; badges are dynamic. */
export const RAIL_GROUPS: RailGroup[] = [
  {
    heading: "STUDY",
    items: [
      { id: "database", label: "Database", icon: "Db" },
      { id: "game", label: "Game", icon: "Gm" },
      { id: "tree", label: "Opening tree", icon: "Tr" },
      { id: "search", label: "Position search", icon: "Ps" },
    ],
  },
  {
    heading: "COACH",
    items: [
      { id: "home", label: "Home", icon: "Hm" },
      { id: "explain", label: "Explain", icon: "Ex" },
      { id: "profile", label: "Profile", icon: "Pf" },
      { id: "prep", label: "Opponent prep", icon: "Op" },
    ],
  },
  {
    heading: "TRAIN",
    items: [
      { id: "train", label: "Openings SRS", icon: "Sr" },
      { id: "tactics", label: "Tactics", icon: "Tc" },
      { id: "endgames", label: "Endgames", icon: "Eg" },
    ],
  },
  {
    heading: "DATA IN / OUT",
    items: [
      { id: "import", label: "Import PGN / SCID", icon: "Im" },
      { id: "twic", label: "TWIC ingest", icon: "Tw" },
      { id: "syncs", label: "Account syncs", icon: "Ac" },
      { id: "jobs", label: "Jobs", icon: "Jb" },
    ],
  },
];

export const RAIL_FOOTER: RailItem[] = [
  { id: "settings", label: "Settings", icon: "St" },
  { id: "help", label: "Help & tour", icon: "?" },
];

/**
 * Per-screen navigation params (round-2 "claim → evidence" contract).
 * `navigate(view, params)` carries these alongside the ViewId; each screen
 * reads only its own keys and ignores the rest. Params are one-shot: they
 * describe how the screen should open, not persistent screen state.
 */
export interface ViewParams {
  /** Profile: claim id to pre-select in the evidence aside
   * ("motif:<Kind>:missed" | "motif:<Kind>:allowed" | "structure:<flag>"). */
  claim?: string;
  /** Profile: player name to auto-build as the self subject (deep links). */
  player?: string;
  /** Prep: opponent name to prefill in step 1. */
  opponent?: string;
}

/**
 * The active screen's keyboard hints for the status strip's right cell
 * (design/handoff-2 §Interactions). Only keys that actually work are
 * listed — null means the screen has no shortcuts (no cell rendered).
 */
export function viewKeyHints(view: ViewId): string | null {
  switch (view) {
    case "game":
      return "← → step · ↑ ↓ jump 5 · f flip · e explain";
    case "train":
      // Openings SRS: ⏎ submits the typed move, 1–4 grade after reveal.
      return "1–4 grade · ⏎ submit";
    default:
      // No other screen has working shortcuts yet — an unearned hint would
      // be a fake affordance.
      return null;
  }
}

/** "121438" → "121k"; small counts stay exact. */
export function formatCount(n: number): string {
  if (n >= 10_000) return `${Math.floor(n / 1000)}k`;
  return String(n);
}

/**
 * Profile findings: motifs with missed opportunities or allowed tactics,
 * plus structures scoring under 50% (the same weakness rules the prep
 * strip uses). Null (no badge) until a profile has been built.
 */
export function profileFindings(profile: PlayerProfile | null): number | null {
  if (!profile) return null;
  const motifs = profile.motifs.filter((m) => m.missed + m.allowed > 0).length;
  const structures = profile.structures.filter((s) => s.score_pct < 50 && s.games >= 2).length;
  return motifs + structures;
}

/** Rail badge per item id; null = show nothing (never a fake number). */
export function railBadge(
  id: RailItem["id"],
  data: {
    dbGames: number | null;
    explainOn: boolean;
    profile: PlayerProfile | null;
    train: TrainSummary | null;
    tactics: TacticsState | null;
    jobs: JobsStatus | null;
  },
): string | null {
  switch (id) {
    case "database":
      return data.dbGames !== null ? formatCount(data.dbGames) : null;
    case "explain":
      return data.explainOn ? "on" : "off";
    case "profile": {
      const n = profileFindings(data.profile);
      return n !== null ? `${n} finding${n === 1 ? "" : "s"}` : null;
    }
    case "train": {
      if (!data.train) return null;
      const due = data.train.white.due + data.train.black.due;
      return `${due} due`;
    }
    case "tactics": {
      if (!data.tactics || data.tactics.puzzles === 0) return null;
      // The tactics table has no due concept; attempts are the real
      // progress number we have.
      return data.tactics.attempts > 0
        ? `${formatCount(data.tactics.attempts)} att`
        : `${formatCount(data.tactics.puzzles)}`;
    }
    case "jobs": {
      if (!data.jobs) return null;
      const { pending, running, done } = data.jobs;
      if (running > 0) return `${running} running`;
      if (pending > 0) return `${pending} pending`;
      return done > 0 ? formatCount(done) : null;
    }
    default:
      return null;
  }
}
