/**
 * UI-session persistence (maintainer request, run 10): what the app needs
 * to reopen exactly where it was closed. The blob lives in the open
 * database's meta table (ui_session_get/set IPC) so it travels with the
 * database file; the last-database PATH itself is remembered Rust-side
 * (last_database IPC) because it must be known before any db is open.
 *
 * The game pointer (id/ply/orientation) is NOT here — home.rs already
 * maintains it live under the `last_game` meta key; restore reads that.
 */
import type { ViewId } from "./shell";
import type { DbScreenState } from "./dbScreenState";

/** Bump when the shape changes incompatibly; unknown versions are ignored. */
const SESSION_VERSION = 1;

export interface UiSession {
  version: number;
  /** Screen the user was on. */
  view: ViewId;
  /** Database-screen filter/page/scroll snapshot. */
  dbScreen: DbScreenState;
}

const VIEW_IDS: readonly ViewId[] = [
  "home",
  "database",
  "game",
  "tree",
  "search",
  "profile",
  "prep",
  "train",
  "triage",
  "tactics",
  "endgames",
  "play",
  "import",
  "twic",
  "syncs",
  "jobs",
  "settings",
];

export function serializeSession(view: ViewId, dbScreen: DbScreenState): string {
  const s: UiSession = { version: SESSION_VERSION, view, dbScreen };
  return JSON.stringify(s);
}

/**
 * Parse a stored blob; null on anything unusable (corrupt JSON, foreign
 * version, unknown view). Restore is best-effort by design — a bad blob
 * must land the user on Home, never on a broken screen.
 */
export function parseSession(json: string | null | undefined): UiSession | null {
  if (!json) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;
  const s = raw as Partial<UiSession>;
  if (s.version !== SESSION_VERSION) return null;
  if (!s.view || !VIEW_IDS.includes(s.view)) return null;
  const d = s.dbScreen;
  const dbScreen: DbScreenState = {
    player: typeof d?.player === "string" ? d.player : "",
    eco: typeof d?.eco === "string" ? d.eco : "",
    result: typeof d?.result === "string" ? d.result : "",
    page: typeof d?.page === "number" && d.page >= 0 ? Math.floor(d.page) : 0,
    scrollTop: typeof d?.scrollTop === "number" && d.scrollTop >= 0 ? d.scrollTop : 0,
    selectedGameId: typeof d?.selectedGameId === "number" ? d.selectedGameId : null,
  };
  return { version: SESSION_VERSION, view: s.view, dbScreen };
}

/**
 * True when the launch hash contains a deep link that must override
 * session restore (never persisted, per the existing contract).
 */
export function hasDeepLinkOverride(hash: string): boolean {
  const h = new URLSearchParams(hash.replace(/^#/, ""));
  return h.has("db") || h.has("game") || h.has("screen");
}
