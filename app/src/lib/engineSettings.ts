/**
 * Engine-manager display logic (run 10, settings/EngineSection.tsx):
 * verification/status lines and input parsing, kept pure so they are
 * unit-testable without Tauri.
 */
import type { TbInfo } from "./endgame";
import type { EngineIdentity } from "./engine";

/** One line describing a successful handshake. A binary that answered
 * `uciok` without an `id name` still verified — say so honestly. */
export function verifiedLine(id: EngineIdentity): string {
  return id.name !== null
    ? `${id.name} — UCI handshake OK`
    : "UCI handshake OK, but the engine sent no id name";
}

/** One line describing a failed handshake; the raw error is preserved. */
export function verifyFailedLine(error: unknown): string {
  return `Not usable: ${String(error)}`;
}

/**
 * The Syzygy row's status line. `configuredDir` is the localStorage
 * override ("" = automatic resolution via KIBITZ_SYZYGY / testdata).
 */
export function tbStatusLine(info: TbInfo | null, configuredDir: string): string {
  const source =
    configuredDir.trim() !== "" ? `configured: ${configuredDir.trim()}` : "automatic (KIBITZ_SYZYGY, else testdata/syzygy)";
  if (info === null) return `${source} — status unknown`;
  if (!info.available) return `not configured — ${source}`;
  return `up to ${info.largest ?? "?"} pieces · ${source}`;
}

/**
 * Parse a node-budget input: a positive integer, tolerating thousands
 * separators ("2,000,000" / "2_000_000" / spaces). Null = not a budget.
 */
export function parseNodesInput(s: string): number | null {
  const t = s.trim().replace(/[,_\s]/g, "");
  if (!/^\d+$/.test(t)) return null;
  const n = parseInt(t, 10);
  return Number.isSafeInteger(n) && n > 0 ? n : null;
}
