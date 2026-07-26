import { useCallback, useEffect, useRef, useState } from "react";
import {
  annotateGame,
  exportGamePgn,
  jobsStatus,
  reanalyzeGame,
  runJobs,
  type JobsStatus,
} from "./lib/db";

interface GameToolsProps {
  /** Database id of the loaded game. */
  gameId: number;
  /** Reload the current game (annotations/evals may have changed). */
  onReload: () => void;
}

/**
 * Per-game database actions (run-4 goal 2 + verdicts 3d/4): static
 * annotation, full re-analysis enqueueing, the user-initiated job runner
 * with a polled status strip, and annotated-PGN export. The export uses a
 * copy-to-clipboard modal: no file-dialog Tauri plugin is bundled and
 * adding one is out of scope for this pass.
 */
export default function GameTools({ gameId, onReload }: GameToolsProps) {
  const [jobs, setJobs] = useState<JobsStatus | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [exportText, setExportText] = useState<string | null>(null);
  /** True once a run was started, so its completion triggers one reload. */
  const wasActive = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setJobs(await jobsStatus());
    } catch {
      setJobs(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, gameId]);

  // Auto-poll every 2s while work remains or the worker is running.
  const active = jobs !== null && (jobs.workerActive || jobs.pending > 0 || jobs.running > 0);
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [active, refresh]);

  // When the worker finishes, reload the game: fold-back may have
  // rewritten the stored annotations, and fresh evals landed.
  useEffect(() => {
    if (!jobs) return;
    if (jobs.workerActive) {
      wasActive.current = true;
    } else if (wasActive.current) {
      wasActive.current = false;
      setMsg(`Job run finished: ${jobs.done} done, ${jobs.failed} failed.`);
      onReload();
    }
  }, [jobs, onReload]);

  const doAnnotate = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const r = await annotateGame(gameId);
      setMsg(
        `Annotated: ${r.positionsAnalyzed} positions, ${r.commentsAdded} comments, ` +
          `${r.jobsEnqueued} engine check(s) enqueued.`,
      );
      await refresh();
      onReload();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doReanalyze = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const n = await reanalyzeGame(gameId);
      setMsg(`Enqueued ${n} position evals (press Run engine jobs to execute).`);
      await refresh();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doRunJobs = async () => {
    setMsg(null);
    try {
      await runJobs();
      wasActive.current = true;
      await refresh();
    } catch (e) {
      setMsg(String(e));
    }
  };

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setMsg("PGN copied to clipboard.");
    } catch {
      setMsg("Clipboard unavailable — select the text and copy manually.");
    }
  };

  const doExport = async () => {
    setMsg(null);
    try {
      const pgn = await exportGamePgn(gameId);
      setExportText(pgn);
      await copyToClipboard(pgn);
    } catch (e) {
      setMsg(String(e));
    }
  };

  return (
    <div className="gametools">
      <div className="gametools-row">
        <button onClick={() => void doAnnotate()} disabled={busy} title="Static Silman annotation (no engine)">
          Annotate game
        </button>
        <button
          onClick={() => void doReanalyze()}
          disabled={busy}
          title="Enqueue a bounded engine eval for every mainline position"
        >
          Re-analyze game
        </button>
        <button
          onClick={() => void doRunJobs()}
          disabled={jobs?.workerActive ?? false}
          title="Run all pending engine jobs, then fold verdicts back into the annotations"
        >
          Run engine jobs
        </button>
        <button onClick={() => void doExport()} title="Export this game as annotated PGN">
          Export PGN
        </button>
      </div>
      {jobs && (
        <div className="jobs-strip">
          <span>
            jobs: {jobs.pending} pending · {jobs.running} running · {jobs.done} done ·{" "}
            {jobs.failed} failed
          </span>
          {jobs.workerActive && <span className="running">running…</span>}
          {jobs.engine && <span className="jobs-engine">engine: {jobs.engine}</span>}
        </div>
      )}
      {msg && <div className="msg">{msg}</div>}

      {exportText !== null && (
        <div className="modal-overlay" onClick={() => setExportText(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Exported PGN — game #{gameId}</h3>
            <textarea readOnly value={exportText} spellCheck={false} />
            <div className="modal-buttons">
              <button onClick={() => void copyToClipboard(exportText)}>Copy</button>
              <button onClick={() => setExportText(null)}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
