/**
 * Jobs (DATA IN/OUT): the engine job queue — counts, the worker toggle
 * (the only user-initiated engine entry point besides explicit analysis)
 * and the most recent engine identity. Progress is polled app-wide.
 */
import type { JobsStatus } from "./lib/db";

interface JobsViewProps {
  jobs: JobsStatus | null;
  running: boolean;
  onRunJobs: () => void;
  error: string | null;
}

export default function JobsView({ jobs, running, onRunJobs, error }: JobsViewProps) {
  return (
    <div className="page jobs-view">
      <h2 className="page-title">Jobs</h2>
      <div className="panel-box">
        {jobs ? (
          <table className="data-table jobs-table">
            <tbody>
              <tr>
                <td>pending</td>
                <td className="num">{jobs.pending.toLocaleString()}</td>
              </tr>
              <tr>
                <td>running</td>
                <td className="num">{jobs.running.toLocaleString()}</td>
              </tr>
              <tr>
                <td>done</td>
                <td className="num">{jobs.done.toLocaleString()}</td>
              </tr>
              <tr>
                <td>failed</td>
                <td className="num">{jobs.failed.toLocaleString()}</td>
              </tr>
            </tbody>
          </table>
        ) : (
          <p className="page-prose">Open a database to see its job queue.</p>
        )}
        <div className="row-gap">
          <button
            className="btn"
            onClick={onRunJobs}
            disabled={!jobs || running || jobs.pending === 0}
          >
            {running ? "Worker running…" : "Run pending jobs"}
          </button>
          {jobs?.engine && <span className="settings-note">last engine: {jobs.engine}</span>}
        </div>
        {error && <div className="error">{error}</div>}
        <p className="page-prose">
          Everything the engine does goes through this queue — annotate confirmations, re-analyze
          passes, batch evals. Nothing runs until you start the worker.
        </p>
      </div>
    </div>
  );
}
