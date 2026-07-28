/**
 * Thin typed wrapper over the network-ingestion IPC surface
 * (src-tauri/src/netops.rs): the TWIC catalog/download commands, the
 * account-sync commands, worker progress polling, and the rail badge
 * data. Pure helpers live at the bottom so they stay unit-testable
 * without a Tauri runtime.
 */
import { invoke } from "@tauri-apps/api/core";
import { utcDateTimeLocal } from "./time";

/* ---- TWIC catalog ---- */

export interface TwicCatalogRow {
  issue: number;
  imported: boolean;
  /** Games imported from this issue; null when not imported. */
  games: number | null;
  /** Approximate publication Monday "YYYY-MM-DD" — display as "approx". */
  approxDate: string;
}

export interface TwicCatalog {
  /** Earliest issue the TWIC zip archive serves. */
  firstAvailable: number;
  latestImported: number | null;
  /** max(imported, probe-confirmed); null until known. */
  latestKnown: number | null;
  /** One row per issue, newest first; empty until latestKnown is known. */
  rows: TwicCatalogRow[];
  autoSync: boolean;
  noticeAcknowledged: boolean;
  /** The exact kibitz-db FIRST_RUN_NOTICE text for the acknowledge dialog. */
  firstRunNotice: string;
}

export interface TwicRefresh {
  latestKnown: number | null;
  /** HEAD requests actually issued (shown for honesty). */
  requests: number;
}

export function twicCatalog(): Promise<TwicCatalog> {
  return invoke<TwicCatalog>("twic_catalog");
}

/** Explicit user action only — issues a handful of HEAD probes (cap 12). */
export function twicRefreshCatalog(): Promise<TwicRefresh> {
  return invoke<TwicRefresh>("twic_refresh_catalog");
}

/** Start the background download of the given issues; returns the count queued. */
export function twicDownload(issues: number[]): Promise<number> {
  return invoke<number>("twic_download", { issues });
}

export function twicSetAutoSync(enabled: boolean): Promise<void> {
  return invoke<void>("twic_set_auto_sync", { enabled });
}

export function twicAckNotice(): Promise<void> {
  return invoke<void>("twic_ack_notice");
}

/** Database-open hook: starts a quiet new-issues sync when the toggle is on. */
export function twicAutoSyncCheck(): Promise<boolean> {
  return invoke<boolean>("twic_auto_sync_check");
}

/* ---- account syncs ---- */

export type SyncService = "lichess" | "chesscom" | "fics";

/** Last-sync report as persisted in meta (see netops.rs). */
export interface SyncReport {
  at?: string;
  gamesImported?: number;
  duplicatesSkipped?: number;
  gamesFailed?: number;
  monthsFetched?: number;
  year?: number;
  month?: number | null;
  savedArchive?: string | null;
  error?: string;
}

export interface ServiceAccount {
  username: string | null;
  lastReport: SyncReport | null;
}

export interface SyncAccounts {
  lichess: ServiceAccount;
  chesscom: ServiceAccount;
  fics: ServiceAccount;
}

export function syncAccounts(): Promise<SyncAccounts> {
  return invoke<SyncAccounts>("sync_accounts");
}

export function syncSetUsername(service: SyncService, username: string): Promise<void> {
  return invoke<void>("sync_set_username", { service, username });
}

/** Start a sync on the background worker (FICS needs year, optional month). */
export function syncRun(
  service: SyncService,
  username: string,
  year?: number,
  month?: number,
): Promise<void> {
  return invoke<void>("sync_run", { service, username, year, month });
}

/* ---- worker progress / cancel / badges ---- */

export interface NetProgress {
  /** "twic" | "twic-auto" | "lichess" | "chesscom" | "fics". */
  kind: string;
  label: string;
  done: number;
  /** 0 = indeterminate (single-request account syncs). */
  total: number;
  detail: string;
  active: boolean;
  error: string | null;
  /** Labels of jobs waiting behind the current one. */
  queued: string[];
}

export function netProgress(): Promise<NetProgress | null> {
  return invoke<NetProgress | null>("net_progress");
}

/** Cooperative cancel (stops between TWIC issues). False = nothing ran. */
export function netCancel(): Promise<boolean> {
  return invoke<boolean>("net_cancel");
}

export interface NetBadges {
  twicLatestImported: number | null;
  accountsConfigured: number;
}

export function railNetBadges(): Promise<NetBadges> {
  return invoke<NetBadges>("rail_net_badges");
}

/* ---- pure helpers (unit-tested in net.test.ts) ---- */

/** Issue numbers of the catalog rows not yet imported. */
export function missingIssues(rows: readonly TwicCatalogRow[]): number[] {
  return rows.filter((r) => !r.imported).map((r) => r.issue);
}

/** One-line rendering of a stored last-sync report; null when none. The
 * stored `at` is UTC; it renders in the user's local time (audit #10).
 * `zone` is test injection only. */
export function formatReport(report: SyncReport | null, zone?: string): string | null {
  if (!report) return null;
  const at = report.at ? utcDateTimeLocal(report.at, zone) : "unknown time";
  if (report.error) return `Failed (${at}): ${report.error}`;
  let s =
    `Last sync ${at}: ${report.gamesImported ?? 0} imported · ` +
    `${report.duplicatesSkipped ?? 0} duplicates · ${report.gamesFailed ?? 0} failed`;
  if (report.monthsFetched !== undefined) s += ` · ${report.monthsFetched} month(s)`;
  if (report.year !== undefined) {
    s += ` · ${report.year}${report.month ? `-${String(report.month).padStart(2, "0")}` : ""}`;
  }
  return s;
}

/**
 * Status-strip progress cell for a running TWIC job. Account syncs are a
 * single indeterminate request — no honest fraction exists, so they get
 * no strip cell (never a fake percentage).
 */
export function netStripProgress(
  p: NetProgress | null,
): { label: string; fraction: number } | null {
  if (!p || !p.active || p.total === 0) return null;
  if (p.kind !== "twic" && p.kind !== "twic-auto") return null;
  return {
    label: p.kind === "twic-auto" ? "TWIC AUTO-SYNC" : "TWIC DOWNLOAD",
    fraction: Math.max(0, Math.min(1, p.done / p.total)),
  };
}
