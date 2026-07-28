/**
 * Settings — Lichess play section (run 10). Own file so the SettingsView
 * diff stays a two-line import+render (other agents add sections too).
 *
 * The token is a SECRET: it is validated and stored Rust-side (app config
 * dir, own file, 0o600 on unix), never in the database, never logged, and
 * never echoed back — this section only ever shows
 * "configured · ends in …XXXX" plus the username it authenticated as.
 */
import { useCallback, useEffect, useState } from "react";
import {
  lichessTokenClear,
  lichessTokenSet,
  lichessTokenStatus,
  type LichessTokenStatus,
} from "../lib/lichessPlay";

export default function LichessSection() {
  const [status, setStatus] = useState<LichessTokenStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    lichessTokenStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  const save = useCallback(async () => {
    if (draft.trim() === "") return;
    setBusy(true);
    setNote(null);
    try {
      const s = await lichessTokenSet(draft);
      setStatus(s);
      setDraft(""); // the secret leaves the DOM immediately
      setNote(`Token saved — signed in as ${s.username ?? "?"}.`);
    } catch (e) {
      setNote(`Token not saved: ${e}`);
    } finally {
      setBusy(false);
    }
  }, [draft]);

  const clear = useCallback(async () => {
    setBusy(true);
    setNote(null);
    try {
      setStatus(await lichessTokenClear());
      setNote("Token removed. Play streams stop.");
    } catch (e) {
      setNote(`Clear failed: ${e}`);
    } finally {
      setBusy(false);
    }
  }, []);

  const statusText =
    status === null
      ? "unavailable"
      : status.configured
        ? `configured · ends in …${status.tokenTail ?? "????"}` +
          (status.username ? ` · signed in as ${status.username}` : "")
        : "not set";

  return (
    <div className="set-group">
      <div className="set-group-head">LICHESS PLAY</div>
      <div className="set-row">
        <div>
          <div className="set-label">Access token</div>
          <div className="set-help">
            Personal access token with the <b>board:play</b> scope — create one at lichess.org →
            Preferences → API access tokens. Stored in an owner-only file on this machine (never
            in the database, never logged) and used for the Play online screen. While a game
            runs, engine assistance is disabled everywhere on the play screen (lichess Terms of
            Service).
          </div>
        </div>
        <div className="set-value-cell">
          <div className="set-value mono">{statusText}</div>
          <input
            className="set-value mono editing"
            type="password"
            value={draft}
            placeholder="paste token (shown never again)"
            spellCheck={false}
            autoComplete="off"
            aria-label="Lichess access token"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void save()}
          />
        </div>
        <div className="set-actions">
          <button
            className="btn-ghost"
            onClick={() => void save()}
            disabled={busy || draft.trim() === ""}
          >
            {busy ? "Working…" : "Save token"}
          </button>
          {status?.configured && (
            <button className="btn-ghost" onClick={() => void clear()} disabled={busy}>
              Clear token
            </button>
          )}
        </div>
      </div>
      {note && <div className="set-note">{note}</div>}
    </div>
  );
}
