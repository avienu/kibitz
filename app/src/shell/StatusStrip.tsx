/**
 * Status strip (design/handoff-1 §E): engine dot + jobs cells + the
 * progress-cell pattern for a running batch, with a right-aligned SRS
 * nudge. Subscribed app-wide (App polls jobs_status), not per screen.
 */
import type { JobsStatus, TrainSummary } from "../lib/db";

interface StatusStripProps {
  /** True while an interactive engine search runs (analyze_position). */
  engineRunning: boolean;
  /** Resolved engine identity/detail, e.g. "Stockfish 18 · nodes 2,000,000". */
  engineDetail: string;
  jobs: JobsStatus | null;
  /** Progress of the running job batch, 0..1, when a worker is active. */
  batchProgress: { label: string; fraction: number } | null;
  train: TrainSummary | null;
  /** Transient app status message (load results, save confirmations…). */
  message: string;
  /** The active screen's keyboard hints (round-2 §Interactions); null =
   * no shortcuts on this screen, cell absent. */
  keyHints?: string | null;
  onNudge: () => void;
}

export default function StatusStrip({
  engineRunning,
  engineDetail,
  jobs,
  batchProgress,
  train,
  message,
  keyHints,
  onNudge,
}: StatusStripProps) {
  const workerActive = jobs?.workerActive ?? false;
  const engineOn = engineRunning || workerActive;
  const due = train ? train.white.due + train.black.due : 0;
  return (
    <footer className="status-strip">
      <div className="strip-cell">
        <span className={`strip-dot${engineOn ? " on" : ""}`} />
        <span>{engineOn ? "ENGINE RUNNING" : "ENGINE IDLE"}</span>
        <span className="strip-detail">{engineDetail}</span>
      </div>
      <div className="strip-cell">
        <span className="strip-label">JOBS</span>
        <span className="strip-detail">
          {jobs
            ? `${jobs.pending} pending · ${jobs.running} running · ${jobs.done} done · ${jobs.failed} failed`
            : "no database open"}
        </span>
      </div>
      {batchProgress && (
        <div className="strip-cell">
          <span className="strip-label">{batchProgress.label}</span>
          <span className="strip-track">
            <span
              className="strip-fill"
              style={{ width: `${Math.round(batchProgress.fraction * 100)}%` }}
            />
          </span>
          <span className="strip-detail">{Math.round(batchProgress.fraction * 100)}%</span>
        </div>
      )}
      {message && (
        <div className="strip-cell strip-message" title={message}>
          <span className="strip-detail">{message}</span>
        </div>
      )}
      <div className="strip-spacer" />
      {keyHints && (
        <div className="strip-cell strip-hints">
          <span className="strip-detail">{keyHints}</span>
        </div>
      )}
      {due > 0 && (
        <button className="strip-cell strip-nudge" onClick={onNudge}>
          Openings SRS · {due} due today
        </button>
      )}
    </footer>
  );
}
