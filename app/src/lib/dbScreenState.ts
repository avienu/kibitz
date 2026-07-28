/**
 * Database screen session state (run-9 field report 1): the filter /
 * page / scroll / selection state of the Database screen, held in a
 * module-level store so it survives navigating away and back (the screen
 * unmounts on every view switch). Since run 10 the snapshot is ALSO
 * persisted across restarts via the ui_session meta blob (lib/session.ts)
 * — App hydrates the store at launch and subscribes to change events to
 * write it back.
 */

export interface DbScreenState {
  /** Player-name substring filter. */
  player: string;
  /** ECO prefix filter (uppercased by the input). */
  eco: string;
  /** Exact result filter ("" = any). */
  result: string;
  /** Event-name substring filter (run 10). */
  event: string;
  /** Date bounds ("" = unbounded): YYYY, YYYY.MM or YYYY.MM.DD. */
  dateMin: string;
  dateMax: string;
  /** Source-kind filter ("" = any): personal | twic | online | other. */
  sourceKind: string;
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
  event: "",
  dateMin: "",
  dateMax: "",
  sourceKind: "",
  page: 0,
  scrollTop: 0,
  selectedGameId: null,
};

let current: DbScreenState = { ...INITIAL };
const listeners = new Set<(s: DbScreenState) => void>();

function notify() {
  for (const fn of listeners) fn(current);
}

/** Current snapshot (read on mount to restore the screen). */
export function dbScreenState(): DbScreenState {
  return current;
}

/** Subscribe to store changes (App persists them). Returns unsubscribe. */
export function subscribeDbScreenState(fn: (s: DbScreenState) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Replace the whole snapshot (session restore at launch). Silent: the
 * hydrated state is what was already persisted — echoing it back would
 * just rewrite the same blob. */
export function hydrateDbScreenState(s: DbScreenState): void {
  current = { ...s };
}

/** Merge a partial update into the store; returns the new snapshot. */
export function updateDbScreenState(patch: Partial<DbScreenState>): DbScreenState {
  current = { ...current, ...patch };
  notify();
  return current;
}

/** The Clear affordance: back to a pristine screen. */
export function clearDbScreenState(): DbScreenState {
  current = { ...INITIAL };
  notify();
  return current;
}

/** True when Clear would change anything the user can see. */
export function hasActiveFilters(s: DbScreenState): boolean {
  return (
    s.player !== "" ||
    s.eco !== "" ||
    s.result !== "" ||
    s.event !== "" ||
    s.dateMin !== "" ||
    s.dateMax !== "" ||
    s.sourceKind !== "" ||
    s.page !== 0
  );
}
