/**
 * Local rendering of stored UTC timestamps — pure, unit-testable.
 *
 * Every timestamp the database stores is SQLite `datetime('now')`, i.e.
 * UTC "YYYY-MM-DD HH:MM:SS" with no zone marker. Audit #10: rendering
 * those strings verbatim shows tomorrow's (or yesterday's) date whenever
 * UTC has crossed midnight and the user's zone hasn't — Home's Continue
 * card said "opened 2026-07-28" under a header reading Monday 27 July.
 * All user-facing timestamps must go through these helpers.
 *
 * `zone` is an injectable IANA zone for deterministic tests; production
 * callers omit it (the user's local zone).
 */

/** Parse a stored UTC "YYYY-MM-DD HH:MM:SS" timestamp; null if malformed. */
export function parseUtc(ts: string): Date | null {
  if (!/^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}(:\d{2})?$/.test(ts)) return null;
  const d = new Date(ts.replace(" ", "T") + "Z");
  return Number.isNaN(d.getTime()) ? null : d;
}

/** Local "YYYY-MM-DD" for a stored UTC timestamp (falls back to the raw
 * date part when the string is malformed — never invents a date). */
export function utcDateLocal(ts: string, zone?: string): string {
  const d = parseUtc(ts);
  if (!d) return ts.slice(0, 10);
  // en-CA long since it renders ISO "YYYY-MM-DD".
  return d.toLocaleDateString("en-CA", { timeZone: zone });
}

/** Local "YYYY-MM-DD HH:MM" for a stored UTC timestamp. */
export function utcDateTimeLocal(ts: string, zone?: string): string {
  const d = parseUtc(ts);
  if (!d) return ts;
  const date = d.toLocaleDateString("en-CA", { timeZone: zone });
  const time = d.toLocaleTimeString("en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    timeZone: zone,
  });
  return `${date} ${time}`;
}

/** Local weekday name ("Tuesday") for a stored UTC timestamp; null when
 * malformed. */
export function utcWeekdayLocal(ts: string, zone?: string): string | null {
  const d = parseUtc(ts);
  if (!d) return null;
  return d.toLocaleDateString("en-GB", { weekday: "long", timeZone: zone });
}
