/**
 * Opening-tree view model helpers — pure logic, unit-tested in isolation.
 *
 * Audit #2: during an active TWIC sync the moves table rendered the
 * true-empty copy ("No database moves from this position") while the
 * query was still in flight (and would have rendered it on a real error
 * too). The empty-state copy is a product claim about the DATABASE; it
 * may only appear for a successful query that returned zero rows. The
 * in-flight and error states get their own words.
 */

export type TreePhase = "closed" | "loading" | "error" | "settled";

/** Discriminate the moves-table phase from the fetch state. */
export function treePhase(dbOpen: boolean, loading: boolean, error: string | null): TreePhase {
  if (!dbOpen) return "closed";
  if (error) return "error";
  if (loading) return "loading";
  return "settled";
}

/** The moves table's empty-slot copy for a phase. "No database moves…"
 * is reserved for `settled` — a successful query with zero rows. */
export function treeEmptyCopy(phase: TreePhase): string {
  switch (phase) {
    case "closed":
      return "Open a database first.";
    case "loading":
      return "Loading moves from the database…";
    case "error":
      return "Couldn't read the database moves — see the error above.";
    case "settled":
      return "No database moves from this position.";
  }
}

/** Same discrimination for the "Games reaching this position" aside. */
export function reachingEmptyCopy(phase: TreePhase): string {
  switch (phase) {
    case "closed":
      return "Open a database first.";
    case "loading":
      return "Loading games…";
    case "error":
      return "Couldn't load the games list.";
    case "settled":
      return "No games reach this exact position.";
  }
}
