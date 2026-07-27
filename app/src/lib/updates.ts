/**
 * Update checking: the "Check for updates" setting, the IPC wrapper over
 * src-tauri/src/updates.rs (`update_check`), and the pure feed logic
 * (semver compare + platform-key selection) that mirrors what the release
 * pipeline publishes as latest.json (scripts/release/generate_latest_json.mjs).
 *
 * The pure functions exist so the feed format we generate in CI is tested
 * here against a mock latest.json fixture (updates.test.ts) — no network.
 * Keep all `invoke` usage in this module so the UI stays pure.
 */
import { invoke } from "@tauri-apps/api/core";

/* ------------------------------------------------------------------ */
/* Setting: check on launch (default ON), localStorage like engine.ts  */
/* ------------------------------------------------------------------ */

const CHECK_ON_LAUNCH_KEY = "kibitz.updates.checkOnLaunch";
const LAST_RESULT_KEY = "kibitz.updates.lastResult";

export function getSavedUpdateCheck(): boolean {
  // Default ON: absent key means enabled.
  return localStorage.getItem(CHECK_ON_LAUNCH_KEY) !== "off";
}

export function saveUpdateCheck(enabled: boolean): void {
  if (enabled) localStorage.removeItem(CHECK_ON_LAUNCH_KEY);
  else localStorage.setItem(CHECK_ON_LAUNCH_KEY, "off");
}

/* ------------------------------------------------------------------ */
/* IPC wrapper                                                         */
/* ------------------------------------------------------------------ */

export interface UpdateCheckResult {
  /** False until a real updater pubkey ships (pre-release builds). */
  configured: boolean;
  available: boolean;
  current: string;
  version?: string | null;
  notes?: string | null;
  error?: string | null;
}

export interface StoredCheck {
  at: string; // ISO timestamp of the check
  result: UpdateCheckResult;
}

export async function updateCheck(): Promise<UpdateCheckResult> {
  const result = await invoke<UpdateCheckResult>("update_check");
  const stored: StoredCheck = { at: new Date().toISOString(), result };
  localStorage.setItem(LAST_RESULT_KEY, JSON.stringify(stored));
  return result;
}

export function getLastCheck(): StoredCheck | null {
  const raw = localStorage.getItem(LAST_RESULT_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as StoredCheck;
    return parsed && typeof parsed.at === "string" && parsed.result ? parsed : null;
  } catch {
    return null;
  }
}

/**
 * Launch-time hook (called once from main.tsx). Checks only when
 * (a) running inside Tauri, (b) the setting is ON. The backend
 * additionally short-circuits (no network) while the updater pubkey is
 * still the placeholder, so pre-release builds never phone home.
 */
export async function maybeCheckForUpdatesOnLaunch(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return; // vitest / plain browser dev
  if (!getSavedUpdateCheck()) return;
  try {
    await updateCheck();
  } catch {
    // Launch must never be disturbed by an update-check failure.
  }
}

/* ------------------------------------------------------------------ */
/* Pure feed logic (tested against the mock latest.json fixture)       */
/* ------------------------------------------------------------------ */

/** One platform entry in the latest.json feed. */
export interface FeedPlatform {
  signature: string;
  url: string;
}

/** The static updater feed the release pipeline publishes. */
export interface LatestFeed {
  version: string;
  notes?: string;
  pub_date?: string;
  platforms: Record<string, FeedPlatform>;
}

/**
 * Compare two semver-ish versions ("1.2.3", optional leading "v",
 * optional prerelease "-beta.1"). Returns -1 | 0 | 1. Numeric core parts
 * compare numerically; a prerelease sorts *before* its release
 * (1.0.0-beta < 1.0.0); prerelease tails break ties lexically.
 */
export function compareVersions(a: string, b: string): -1 | 0 | 1 {
  const parse = (v: string) => {
    const clean = v.trim().replace(/^v/, "");
    const [core, ...pre] = clean.split("-");
    const nums = core.split(".").map((p) => {
      const n = parseInt(p, 10);
      return Number.isFinite(n) ? n : 0;
    });
    while (nums.length < 3) nums.push(0);
    return { nums, pre: pre.join("-") };
  };
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.nums.length, pb.nums.length); i++) {
    const x = pa.nums[i] ?? 0;
    const y = pb.nums[i] ?? 0;
    if (x !== y) return x < y ? -1 : 1;
  }
  if (pa.pre !== pb.pre) {
    if (pa.pre === "") return 1; // release > its prerelease
    if (pb.pre === "") return -1;
    return pa.pre < pb.pre ? -1 : 1;
  }
  return 0;
}

/**
 * The platform key the Tauri updater looks up in latest.json:
 * `{os}-{arch}`, e.g. "darwin-aarch64", "linux-x86_64".
 */
export function platformKey(os: "darwin" | "linux" | "windows", arch: string): string {
  return `${os}-${arch}`;
}

/**
 * Pick the feed entry for a platform key. Exact key wins; on macOS a
 * "darwin-universal" entry serves both architectures as a fallback.
 */
export function selectPlatform(feed: LatestFeed, key: string): FeedPlatform | null {
  const exact = feed.platforms[key];
  if (exact) return exact;
  if (key.startsWith("darwin-")) return feed.platforms["darwin-universal"] ?? null;
  return null;
}

/**
 * Full offer decision: a strictly newer feed version with an entry for
 * this platform, else null. This mirrors the updater plugin's semantics
 * and validates the latest.json our release pipeline generates.
 */
export function feedOffer(
  feed: LatestFeed,
  currentVersion: string,
  key: string,
): { version: string; url: string; signature: string } | null {
  if (compareVersions(feed.version, currentVersion) <= 0) return null;
  const platform = selectPlatform(feed, key);
  if (!platform) return null;
  return { version: feed.version, url: platform.url, signature: platform.signature };
}
