/**
 * Database storage helpers (maintainer request 2026-07-29): detect a
 * database living inside a cloud-synced folder — the cloud client
 * re-hashes the file on every write (at 1 GB+ it saturates the machine)
 * and live-SQLite-under-cloud-sync risks corruption — and the IPC to
 * move it into app storage.
 */
import { invoke } from "@tauri-apps/api/core";

/** Path segments that mark the major cloud-sync products. */
const CLOUD_MARKERS = [
  "/Library/CloudStorage/", // macOS file-provider mounts (Dropbox, Drive, OneDrive…)
  "/Dropbox/",
  "/Google Drive/",
  "/OneDrive/",
  "/iCloud Drive/",
  "/Mobile Documents/", // iCloud's on-disk name
];

/** True when `path` lives inside a folder a cloud client synchronizes. */
export function isCloudSyncedPath(path: string): boolean {
  return CLOUD_MARKERS.some((m) => path.includes(m));
}

export interface MigrateReport {
  newPath: string;
  /** The old file, left untouched as a backup. */
  oldPath: string;
}

/** Snapshot the open database into app storage and switch to it.
 * Backend refuses while a sync or analysis batch is writing. */
export function migrateDatabaseToAppStorage(): Promise<MigrateReport> {
  return invoke<MigrateReport>("migrate_database_to_app_storage");
}
