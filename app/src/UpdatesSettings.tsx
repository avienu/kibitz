/**
 * Settings → UPDATES group (run-8 packaging item). Kept in its own file so
 * SettingsView only gains a one-line import + render (live-analysis work may
 * touch SettingsView concurrently).
 *
 * Honesty rule: while the updater pubkey is still the checked-in placeholder
 * (pre-release builds), the row says "not configured" — the backend
 * short-circuits without any network call. Rows follow the settings grid
 * (230px label+help | 1fr value | 200px action) via the same classnames.
 */
import { useCallback, useState } from "react";
import {
  getLastCheck,
  getSavedUpdateCheck,
  saveUpdateCheck,
  updateCheck,
  type StoredCheck,
} from "./lib/updates";

function describe(check: StoredCheck | null, checking: boolean): string {
  if (checking) return "checking…";
  if (!check) return "not checked yet";
  const r = check.result;
  const when = new Date(check.at).toLocaleString("en-US");
  if (!r.configured) return "Updater not configured — pre-release build without a signing key.";
  if (r.error) return `${r.error} (last tried ${when})`;
  if (r.available) return `Update available: ${r.version ?? "?"} (you have ${r.current})`;
  return `Up to date (${r.current}, checked ${when})`;
}

export default function UpdatesSettings() {
  const [enabled, setEnabled] = useState(getSavedUpdateCheck);
  const [last, setLast] = useState<StoredCheck | null>(getLastCheck);
  const [checking, setChecking] = useState(false);

  const toggle = useCallback(() => {
    setEnabled((v) => {
      saveUpdateCheck(!v);
      return !v;
    });
  }, []);

  const checkNow = useCallback(async () => {
    setChecking(true);
    try {
      await updateCheck();
    } catch {
      // Expected outside a Tauri window; the row keeps its last state.
    } finally {
      setLast(getLastCheck());
      setChecking(false);
    }
  }, []);

  return (
    <div className="set-group">
      <div className="set-group-head">UPDATES</div>
      <div className="set-row">
        <div>
          <div className="set-label">Check for updates</div>
          <div className="set-help">
            One check against the GitHub release feed at launch — nothing polls in the background.
            Installing an update is always a separate, explicit step.
          </div>
        </div>
        <div className="set-value-cell">
          <div className="set-value">{enabled ? "On at launch" : "Off"}</div>
        </div>
        <div className="set-actions">
          <button className="btn-ghost" onClick={toggle}>
            {enabled ? "Turn off" : "Turn on"}
          </button>
        </div>
      </div>
      <div className="set-row">
        <div>
          <div className="set-label">Status</div>
          <div className="set-help">
            Result of the most recent check. Signed update feeds ship once the release pipeline has
            its keys; until then this row states so.
          </div>
        </div>
        <div className="set-value-cell">
          <div className="set-value">{describe(last, checking)}</div>
        </div>
        <div className="set-actions">
          <button className="btn-ghost" onClick={() => void checkNow()} disabled={checking}>
            {checking ? "Checking…" : "Check now"}
          </button>
        </div>
      </div>
    </div>
  );
}
