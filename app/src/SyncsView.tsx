/**
 * Account syncs screen (run 9 — replaces the CLI-pointer placeholder):
 * per-service cards for Lichess, chess.com and FICS running the existing
 * kibitz-db clients on the shared background network worker, plus an
 * honest ICC note (manual export only — no scriptable API).
 *
 * Honesty rules: usernames persist in the database meta table and the
 * clients' own per-username cursors do the incremental resume; the last
 * sync report (or its error) is shown verbatim from the persisted meta
 * record; a running sync shows an indeterminate state — no fake
 * percentages for a single streaming request; 429 waits are named. FICS
 * carries the ficsgames.org personal-use posture from the client docs.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  formatReport,
  syncAccounts,
  syncRun,
  syncSetUsername,
  type NetProgress,
  type ServiceAccount,
  type SyncAccounts,
  type SyncService,
} from "./lib/net";

interface SyncsViewProps {
  /** App-level poll of the shared network worker (netops.rs). */
  progress: NetProgress | null;
}

interface CardProps {
  service: SyncService;
  title: string;
  blurb: string;
  account: ServiceAccount | null;
  progress: NetProgress | null;
  busy: boolean;
  onRun: (service: SyncService, username: string, year?: number, month?: number) => void;
  /** Extra inputs (FICS year/month) rendered beside the username. */
  extraInputs?: React.ReactNode;
  extraNote?: React.ReactNode;
  runArgs?: () => { year?: number; month?: number } | null;
}

function ServiceCard({
  service,
  title,
  blurb,
  account,
  progress,
  busy,
  onRun,
  extraInputs,
  extraNote,
  runArgs,
}: CardProps) {
  const [username, setUsername] = useState("");
  const seeded = useRef(false);
  useEffect(() => {
    // Seed the input from the persisted username once it arrives.
    if (!seeded.current && account?.username) {
      setUsername(account.username);
      seeded.current = true;
    }
  }, [account]);

  const running = progress?.active && progress.kind === service;
  const lastLine = formatReport(account?.lastReport ?? null);
  const args = runArgs ? runArgs() : {};

  return (
    <div className="sync-card">
      <div className="sync-card-head">{title}</div>
      <p className="sync-blurb">{blurb}</p>
      <div className="sync-row">
        <input
          type="text"
          value={username}
          placeholder={`${title} username`}
          spellCheck={false}
          aria-label={`${title} username`}
          onChange={(e) => setUsername(e.target.value)}
          onBlur={() => {
            // Persist edits (including clearing) without needing a sync.
            if (account && (account.username ?? "") !== username.trim()) {
              syncSetUsername(service, username).catch(() => {});
            }
          }}
        />
        {extraInputs}
        <button
          className="btn-secondary"
          disabled={busy || username.trim() === "" || args === null}
          onClick={() => onRun(service, username.trim(), args?.year, args?.month)}
        >
          {running ? "Syncing…" : "Sync now"}
        </button>
      </div>
      {running && (
        <div className="sync-running">
          <span className="strip-dot on" /> {progress?.detail}
        </div>
      )}
      {!running && progress?.kind === service && progress?.error && (
        <div className="sync-error">Last run failed: {progress.error}</div>
      )}
      {lastLine && (
        <div className={account?.lastReport?.error ? "sync-error" : "sync-report"}>{lastLine}</div>
      )}
      {account?.lastReport?.savedArchive && (
        <div className="sync-note">
          The server returned a bzip2 archive, saved to{" "}
          <code>{account.lastReport.savedArchive}</code> — decompress it with <code>bunzip2</code>{" "}
          and load the PGN via Import PGN / SCID.
        </div>
      )}
      {extraNote}
    </div>
  );
}

export default function SyncsView({ progress }: SyncsViewProps) {
  const [accounts, setAccounts] = useState<SyncAccounts | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [ficsYear, setFicsYear] = useState(String(new Date().getFullYear()));
  const [ficsMonth, setFicsMonth] = useState("");

  const load = useCallback(() => {
    syncAccounts()
      .then((a) => {
        setAccounts(a);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(load, [load]);

  // Reload persisted reports when the shared worker finishes any job.
  const wasActive = useRef(false);
  useEffect(() => {
    const active = progress?.active ?? false;
    if (wasActive.current && !active) load();
    wasActive.current = active;
  }, [progress, load]);

  const busy = progress?.active ?? false;

  const run = useCallback(
    (service: SyncService, username: string, year?: number, month?: number) => {
      setNote(null);
      syncRun(service, username, year, month)
        .then(load) // pick up the persisted username immediately
        .catch((e) => setNote(String(e)));
    },
    [load],
  );

  const ficsArgs = useCallback((): { year?: number; month?: number } | null => {
    const year = parseInt(ficsYear, 10);
    if (!Number.isFinite(year) || year < 1999) return null; // ficsgames.org starts 1999
    if (ficsMonth === "") return { year };
    const month = parseInt(ficsMonth, 10);
    if (!Number.isFinite(month) || month < 1 || month > 12) return null;
    return { year, month };
  }, [ficsYear, ficsMonth]);

  return (
    <div className="page syncs-page">
      {error && (
        <div className="error">{error} — open a database on the Database screen first.</div>
      )}
      {note && <div className="twic-note">{note}</div>}
      {busy && progress && (
        <div className="sync-serial-note">
          One network job at a time (strictly serial): {progress.label} is running.
        </div>
      )}

      <div className="sync-cards">
        <ServiceCard
          service="lichess"
          title="Lichess"
          blurb="Full game export via the Lichess API, resumed incrementally — after the first sync only games newer than the last one are downloaded."
          account={accounts?.lichess ?? null}
          progress={progress}
          busy={busy}
          onRun={run}
        />
        <ServiceCard
          service="chesscom"
          title="chess.com"
          blurb="Monthly archives via the chess.com published-data API, oldest first, resumed incrementally — completed months are never re-fetched (the newest month is re-checked; duplicates are skipped)."
          account={accounts?.chesscom ?? null}
          progress={progress}
          busy={busy}
          onRun={run}
        />
        <ServiceCard
          service="fics"
          title="FICS"
          blurb="FICS games via ficsgames.org (the community archive). One year — or one month — per request; there is no incremental cursor, but re-runs are harmless (duplicates are skipped)."
          account={accounts?.fics ?? null}
          progress={progress}
          busy={busy}
          onRun={run}
          runArgs={ficsArgs}
          extraInputs={
            <>
              <input
                className="sync-year"
                type="number"
                min={1999}
                value={ficsYear}
                aria-label="FICS year"
                onChange={(e) => setFicsYear(e.target.value)}
              />
              <input
                className="sync-month"
                type="number"
                min={1}
                max={12}
                value={ficsMonth}
                placeholder="mm"
                aria-label="FICS month (optional)"
                onChange={(e) => setFicsMonth(e.target.value)}
              />
            </>
          }
          extraNote={
            <p className="sync-note">
              ficsgames.org is a volunteer-run archive with limited bandwidth — requests are
              strictly serial and one year (or month) per click; keep them occasional. Personal
              use only.
            </p>
          }
        />

        <div className="sync-card sync-card-icc">
          <div className="sync-card-head">ICC</div>
          <p className="sync-blurb">
            The Internet Chess Club has no scriptable export API, so Kibitz cannot sync it.
            Export your games manually from the ICC client and load the PGN via{" "}
            <strong>Import PGN / SCID</strong>.
          </p>
        </div>
      </div>

      <p className="twic-footnote">
        All syncs run one request at a time with a descriptive User-Agent; HTTP 429 rate limits
        are respected automatically (a sync may pause for a minute or more). Provenance of every
        imported game — source, exact URL, license, date — is recorded in the database.
      </p>
    </div>
  );
}
