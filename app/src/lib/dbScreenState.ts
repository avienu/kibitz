/**
 * Database screen session state (run-9 field report 1): the filter /
 * page / scroll / selection state of the Database screen, held in a
 * module-level store so it survives navigating away and back (the screen
 * unmounts on every view switch). Deliberately SESSION-scoped — unlike
 * the localStorage helpers in lib/db.ts this is never persisted, so a
 * restart starts clean.
 */

export interface DbScreenState {
  /** Player-name substring filter. */
  player: string;
  /** ECO prefix filter (uppercased by the input). */
  eco: string;
  /** Exact result filter ("" = any). */
  result: string;
  /** 0-based page. */
  page: number;
  /** Scroll offset of the screen's scroll container, in px. */
  scrollTop: number;
  /** Last game opened from the table (row re-highlight), if any. */
  selectedGameId: number | null;
}

const INITIAL: DbScreenState = {
  player: "",
  eco: "",
  result: "",
  page: 0,
  scrollTop: 0,
  selectedGameId: null,
};

let current: DbScreenState = { ...INITIAL };

/** Current snapshot (read on mount to restore the screen). */
export function dbScreenState(): DbScreenState {
  return current;
}

/** Merge a partial update into the store; returns the new snapshot. */
export function updateDbScreenState(patch: Partial<DbScreenState>): DbScreenState {
  current = { ...current, ...patch };
  return current;
}

/** The Clear affordance: back to a pristine screen. */
export function clearDbScreenState(): DbScreenState {
  current = { ...INITIAL };
  return current;
}

/** True when Clear would change anything the user can see. */
export function hasActiveFilters(s: DbScreenState): boolean {
  return s.player !== "" || s.eco !== "" || s.result !== "" || s.page !== 0;
}
