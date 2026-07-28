/**
 * Lichess opening-explorer client (run 10): types, the opt-in toggle
 * (localStorage, OFF by default — network-quiet is a product value), and
 * the debounced, per-FEN-cached loader the Opening-tree screen uses.
 *
 * Rate discipline lives HERE, not in the view: one request per settled
 * position (500 ms debounce), cache hits answer synchronously without
 * touching the network, and stale responses are dropped by sequence
 * number. The actual HTTP rides a Rust proxy command (explorer_fetch,
 * kibitz-db net plumbing + descriptive User-Agent) so webview CORS/CSP
 * differences can never break it.
 */
import { invoke } from "@tauri-apps/api/core";

const TOGGLE_KEY = "kibitz.onlineExplorer";

/** Opt-in state; OFF until the user flips it. */
export function getExplorerEnabled(): boolean {
  return localStorage.getItem(TOGGLE_KEY) === "on";
}

export function saveExplorerEnabled(on: boolean): void {
  if (on) localStorage.setItem(TOGGLE_KEY, "on");
  else localStorage.removeItem(TOGGLE_KEY);
}

/* ---- wire types (subset of the explorer response we render) ---- */

export interface ExplorerMove {
  uci: string;
  san: string;
  white: number;
  draws: number;
  black: number;
  averageRating?: number;
}

export interface ExplorerReply {
  white: number;
  draws: number;
  black: number;
  moves: ExplorerMove[];
}

/** Fetch + parse one position (the raw command returns the JSON body). */
export async function explorerFetch(fen: string): Promise<ExplorerReply> {
  const body = await invoke<string>("explorer_fetch", { fen });
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    throw new Error("lichess explorer sent unparseable data");
  }
  const r = parsed as Partial<ExplorerReply>;
  return {
    white: typeof r.white === "number" ? r.white : 0,
    draws: typeof r.draws === "number" ? r.draws : 0,
    black: typeof r.black === "number" ? r.black : 0,
    moves: Array.isArray(r.moves) ? (r.moves as ExplorerMove[]) : [],
  };
}

/* ---- loader: debounce + in-memory cache + stale-drop ---- */

export const EXPLORER_DEBOUNCE_MS = 500;

export type ExplorerState =
  | { kind: "loading" }
  | { kind: "data"; reply: ExplorerReply }
  | { kind: "error"; message: string };

export interface ExplorerLoader {
  /** Ask for `fen`; `cb` fires with loading → data|error. A cached FEN
   * answers synchronously with data and no network request. */
  request(fen: string, cb: (s: ExplorerState) => void): void;
  /** Cancel the pending debounce (view unmount / toggle off). */
  cancel(): void;
  /** Cached FEN count (tests + the pane's honesty footer). */
  cacheSize(): number;
}

/** Build a loader over any fetch function (tests inject a fake). */
export function createExplorerLoader(
  fetchFn: (fen: string) => Promise<ExplorerReply>,
  debounceMs: number = EXPLORER_DEBOUNCE_MS,
): ExplorerLoader {
  const cache = new Map<string, ExplorerReply>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let seq = 0;

  return {
    request(fen, cb) {
      seq++;
      const mySeq = seq;
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      const hit = cache.get(fen);
      if (hit !== undefined) {
        cb({ kind: "data", reply: hit });
        return;
      }
      cb({ kind: "loading" });
      timer = setTimeout(() => {
        timer = null;
        fetchFn(fen)
          .then((reply) => {
            cache.set(fen, reply);
            if (seq === mySeq) cb({ kind: "data", reply });
          })
          .catch((e) => {
            if (seq === mySeq) cb({ kind: "error", message: String(e) });
          });
      }, debounceMs);
    },
    cancel() {
      seq++;
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
    },
    cacheSize() {
      return cache.size;
    },
  };
}

/** App-wide loader: the cache survives screen unmounts for the session. */
export const explorerLoader: ExplorerLoader = createExplorerLoader(explorerFetch);

/* ---- pure display helpers ---- */

/** Total games in a reply (or one of its moves). */
export function explorerTotal(r: { white: number; draws: number; black: number }): number {
  return r.white + r.draws + r.black;
}
