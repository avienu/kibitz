/**
 * Filter-input helpers for the Database and Position-search screens
 * (run 10 — the "Event / Date / Source / Elo / Result" chips made real).
 *
 * Pure logic — no DOM, no Tauri. Validation mirrors the backend
 * (src-tauri/src/browse.rs parse_date_bound): a date bound is "YYYY",
 * "YYYY.MM" or "YYYY.MM.DD". Invalid bounds are never sent to the
 * backend; the input shows an inline hint instead.
 */

/** True when `s` is empty or a well-formed date bound. */
export function isValidDateBound(s: string): boolean {
  const t = s.trim();
  if (t === "") return true;
  const m = t.match(/^(\d{4})(?:\.(\d{2})(?:\.(\d{2}))?)?$/);
  if (!m) return false;
  const month = m[2] ? parseInt(m[2], 10) : null;
  const day = m[3] ? parseInt(m[3], 10) : null;
  if (month !== null && (month < 1 || month > 12)) return false;
  if (day !== null && (day < 1 || day > 31)) return false;
  return true;
}

/** Inline hint for a pair of date-bound inputs; null when both are fine. */
export function dateRangeHint(min: string, max: string): string | null {
  if (!isValidDateBound(min) || !isValidDateBound(max)) {
    return "Dates: YYYY, YYYY.MM or YYYY.MM.DD";
  }
  return null;
}

/** The date bound to SEND: the trimmed value when valid and non-empty,
 * else undefined (an invalid bound must never reach the backend). */
export function dateBoundParam(s: string): string | undefined {
  const t = s.trim();
  return t !== "" && isValidDateBound(t) ? t : undefined;
}

/** Parse an Elo input: integer 0..4000, else undefined (not sent). */
export function eloParam(s: string): number | undefined {
  const t = s.trim();
  if (!/^\d{1,4}$/.test(t)) return undefined;
  const n = parseInt(t, 10);
  return n <= 4000 ? n : undefined;
}

/** The Database screen's source-kind choices, in duplicate-priority order. */
export const SOURCE_KINDS = ["personal", "twic", "online", "other"] as const;
