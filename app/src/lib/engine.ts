/**
 * Thin typed wrapper over the Tauri IPC surface of the Rust UCI manager.
 * Keep all `invoke`/`listen` usage here so the rest of the UI stays pure.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TbInfo } from "./endgame";
import type { EngineDone, EngineInfo } from "./engineView";

const USER_PATH_KEY = "kibitz.enginePath";
const NODES_KEY = "kibitz.engineNodes";
const TB_DIR_KEY = "kibitz.tbDir";

export const DEFAULT_NODES = 2_000_000;

export function getSavedEnginePath(): string {
  return localStorage.getItem(USER_PATH_KEY) ?? "";
}

export function saveEnginePath(path: string): void {
  if (path.trim() === "") localStorage.removeItem(USER_PATH_KEY);
  else localStorage.setItem(USER_PATH_KEY, path.trim());
}

export function getSavedNodes(): number {
  const raw = localStorage.getItem(NODES_KEY);
  const n = raw ? parseInt(raw, 10) : NaN;
  return Number.isFinite(n) && n > 0 ? n : DEFAULT_NODES;
}

export function saveNodes(nodes: number): void {
  localStorage.setItem(NODES_KEY, String(nodes));
}

/** Resolve the engine path the Rust side would use (user > env > repo > PATH). */
export function resolveEnginePath(userPath: string): Promise<string> {
  return invoke<string>("resolve_engine_path", {
    userPath: userPath.trim() === "" ? null : userPath.trim(),
  });
}

/* ---- engine manager (run 10, Settings) ---- */

/** `id name` handshake result for the resolved binary. */
export interface EngineIdentity {
  path: string;
  /** "Stockfish 17.1", or null when the binary spoke UCI without one. */
  name: string | null;
}

/** Validate the resolved binary via the `uci` handshake (no search runs —
 * an explicit Settings action). */
export function engineIdentify(userPath: string): Promise<EngineIdentity> {
  return invoke<EngineIdentity>("engine_identify", {
    userPath: userPath.trim() === "" ? null : userPath.trim(),
  });
}

/** User-configured Syzygy directory override ("" = resolve automatically:
 * KIBITZ_SYZYGY, else the repo-local testdata/syzygy). Same localStorage
 * pattern as the engine-path override. */
export function getSavedTbDir(): string {
  return localStorage.getItem(TB_DIR_KEY) ?? "";
}

export function saveTbDir(dir: string): void {
  if (dir.trim() === "") localStorage.removeItem(TB_DIR_KEY);
  else localStorage.setItem(TB_DIR_KEY, dir.trim());
}

/** Current tablebase status (no database needed). */
export function tablebaseStatus(): Promise<TbInfo> {
  return invoke<TbInfo>("tablebase_status");
}

/** Set ("" clears) the Syzygy directory override backend-side and
 * re-resolve; returns the resulting status. Callers persist via
 * saveTbDir on success. */
export function setTablebaseDir(dir: string): Promise<TbInfo> {
  return invoke<TbInfo>("set_tablebase_dir", {
    dir: dir.trim() === "" ? null : dir.trim(),
  });
}

/**
 * Start `go nodes N` on `fen`. Resolves when the search has been accepted;
 * progress arrives via onEngineInfo / onEngineDone events.
 */
export function analyzePosition(fen: string, nodes: number, userPath: string): Promise<void> {
  return invoke<void>("analyze_position", {
    fen,
    nodes,
    userPath: userPath.trim() === "" ? null : userPath.trim(),
  });
}

/** Start `go infinite` on `fen` (live analysis — explicit user action). */
export function analyzeLive(fen: string, userPath: string): Promise<void> {
  return invoke<void>("analyze_position", {
    fen,
    infinite: true,
    userPath: userPath.trim() === "" ? null : userPath.trim(),
  });
}

export function stopAnalysis(): Promise<void> {
  return invoke<void>("stop_analysis");
}

/** `engine-info` event payload: the parsed info line stamped with the FEN
 * of the position the search was started on (src-tauri/src/lib.rs
 * InfoPayload). Consumers MUST attribute the score/PV to `fen` — infos
 * from a just-stopped search keep streaming briefly after a restart, and
 * attributing them to the newly shown position flips the score's sign. */
export interface EngineInfoEvent extends EngineInfo {
  fen: string;
}

export function onEngineInfo(cb: (info: EngineInfoEvent) => void): Promise<UnlistenFn> {
  return listen<EngineInfoEvent>("engine-info", (e) => cb(e.payload));
}

export function onEngineDone(cb: (done: EngineDone) => void): Promise<UnlistenFn> {
  return listen<EngineDone>("engine-done", (e) => cb(e.payload));
}
