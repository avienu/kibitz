/**
 * Home (Direction A) view model — design/handoff-2 §Home. Pure logic:
 * greeting clause rules, source-tag tones, findings prose, duration and
 * date formatting. No DOM, no Tauri — unit-testable in isolation.
 *
 * Honesty rules encoded here (maintainer rulings):
 * - the commitment clause renders ONLY when a commitment label is set;
 * - "no prep started for X yet" ONLY when the commitment names an opponent
 *   AND prep_state has no entry for them;
 * - absent data produces absent UI, never invented widgets.
 */

import type { Commitment, HomeFinding, HomeSummary, PrepEntry } from "./db";
import { utcWeekdayLocal } from "./time";

/** "Sunday, 26 July" — the serif greeting date (assembled by hand: ICU's
 * en-GB long format drops the comma the design shows). */
export function greetingDate(now: Date): string {
  const weekday = now.toLocaleDateString("en-GB", { weekday: "long" });
  const month = now.toLocaleDateString("en-GB", { month: "long" });
  return `${weekday}, ${now.getDate()} ${month}`;
}

/**
 * The greeting's commitment clause, or null when it must be absent.
 * E.g. "Club night Thursday — no prep started for R. Halvorsen yet."
 */
export function commitmentClause(
  commitment: Commitment | null,
  prepState: readonly PrepEntry[],
): string | null {
  if (!commitment?.label) return null;
  const { label, opponent } = commitment;
  if (opponent && !prepState.some((p) => p.opponent === opponent)) {
    return `${label} — no prep started for ${opponent} yet.`;
  }
  return `${label}.`;
}

/** Source-tag tone (shared with the Database table): CSS suffix per kind.
 * personal=accent · twic=info · lichess=violet · chess.com=good. The db
 * stores lichess/chess.com both as kind "online"; the source name breaks
 * the tie. */
export function sourceTagTone(sourceKind: string, sourceName: string): string {
  switch (sourceKind) {
    case "personal":
      return "accent";
    case "twic":
      return "info";
    case "online": {
      const n = sourceName.toLowerCase();
      if (n.includes("lichess")) return "violet";
      if (n.includes("chess.com") || n.includes("chesscom")) return "good";
      return "dim";
    }
    default:
      return "dim";
  }
}

/** "NEW SINCE FRIDAY" — weekday of the oldest new-game import in the list;
 * null when there are no new games (the panel is then absent). The stored
 * timestamp is UTC; the weekday is the USER's (audit #10 — a late-evening
 * local import must not be labelled with tomorrow's name). `zone` is
 * test injection only. */
export function newSinceLabel(
  summary: Pick<HomeSummary, "newGames">,
  zone?: string,
): string | null {
  if (summary.newGames.length === 0) return null;
  const oldest = summary.newGames.reduce((a, b) => (a.importedAt <= b.importedAt ? a : b));
  const weekday = utcWeekdayLocal(oldest.importedAt, zone);
  if (!weekday) return "NEW THIS WEEK";
  return `NEW SINCE ${weekday.toUpperCase()}`;
}

/**
 * The new-games panel footer (audit #11): "N games this week" may only
 * count personal/online-source games — a bulk TWIC import is not "your
 * week". Bulk arrivals are still named, honestly and separately.
 */
export function newGamesFoot(
  summary: Pick<HomeSummary, "newGames" | "newGamesTotal" | "newGamesPersonalTotal">,
): string {
  const personal = summary.newGamesPersonalTotal;
  const bulk = summary.newGamesTotal - personal;
  const n = (v: number) => v.toLocaleString("en-US");
  const showingLatest = summary.newGamesTotal > summary.newGames.length ? " · showing latest" : "";
  if (personal > 0) {
    const yours = `${n(personal)} personal/online game${personal === 1 ? "" : "s"} this week`;
    const extra = bulk > 0 ? ` · plus ${n(bulk)} from bulk imports` : "";
    return `${yours}${extra}${showingLatest}`;
  }
  return `${n(bulk)} game${bulk === 1 ? "" : "s"} from bulk imports this week — none from your own play${showingLatest}`;
}

/** One line for the Running panel's network row (audit #11: the panel
 * said "the engine is cold" while a TWIC sync was actively running);
 * null when no network job is active. */
export function networkRunningLine(p: {
  active: boolean;
  label: string;
  done: number;
  total: number;
} | null): string | null {
  if (!p?.active) return null;
  return p.total > 0 ? `${p.label} · ${p.done} / ${p.total}` : p.label;
}

/** Serif paragraph naming the top two findings in plain language. */
export function findingsProse(findings: readonly HomeFinding[]): string | null {
  if (findings.length === 0) return null;
  const name = (f: HomeFinding) => f.label.replace(/\s*—\s*/, " — ").toLowerCase();
  if (findings.length === 1) {
    return `Your biggest leak is ${name(findings[0])} (${findings[0].value}).`;
  }
  return (
    `Your biggest leaks are ${name(findings[0])} (${findings[0].value}) ` +
    `and ${name(findings[1])} (${findings[1].value}).`
  );
}

/** Role-dot tone per claim kind: motif claims are weaknesses (bad), the
 * structure claim is a score reading (good hue per the design's rows). */
export function findingDotTone(claimId: string): "bad" | "good" {
  return claimId.startsWith("structure:") ? "good" : "bad";
}

/**
 * The fully-degraded Home state (short honest list instead of widgets):
 * nothing due, no new games, no findings, no commitment.
 */
export function isDegraded(
  summary: Pick<HomeSummary, "dueSrs" | "newGames" | "findingsAvailable">,
  commitment: Commitment | null,
): boolean {
  return (
    summary.dueSrs === 0 &&
    summary.newGames.length === 0 &&
    !summary.findingsAvailable &&
    !commitment?.label
  );
}

/** "~2 h 10 m" / "~45 m" / "~20 s" — batch time estimates. */
export function fmtDurationMs(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `~${Math.max(1, s)} s`;
  const m = Math.round(s / 60);
  if (m < 60) return `~${m} m`;
  const h = Math.floor(m / 60);
  const rem = m % 60;
  return rem === 0 ? `~${h} h` : `~${h} h ${rem} m`;
}

/** "1–50 of 121,438" — the filter bar's right-aligned range readout. */
export function rangeReadout(offset: number, shown: number, total: number): string {
  if (total === 0) return "0 games";
  const from = offset + 1;
  const to = offset + shown;
  return `${from.toLocaleString("en-US")}–${to.toLocaleString("en-US")} of ${total.toLocaleString("en-US")}`;
}
