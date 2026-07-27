/**
 * Home — Direction A, coach-first (design/handoff-2 §Home; maintainer
 * ruling: Direction A ONLY, no A/B switch). Greeting row, three action
 * cards (Continue / Due today / Prep an opponent), findings panel beside
 * the "New since …" + "Running" column.
 *
 * Every element is honest-only: the Continue card exists only when a game
 * was actually opened, findings come only from a cached profile, the
 * tactics numeral is never faked (the queue is endless), and when nothing
 * at all is due the screen degrades to a short honest list — never padded
 * with invented widgets.
 *
 * `HomeContent` is the pure renderer (fixture-testable); the default
 * export fetches the live data.
 */
import { useCallback, useEffect, useState } from "react";
import {
  commitmentGet,
  homeSummary,
  prepStateGet,
  type Commitment,
  type HomeSummary,
  type PrepEntry,
} from "./lib/db";
import {
  commitmentClause,
  findingDotTone,
  findingsProse,
  greetingDate,
  isDegraded,
  newSinceLabel,
  sourceTagTone,
} from "./lib/home";
import type { ViewId, ViewParams } from "./lib/shell";

export interface HomeData {
  summary: HomeSummary;
  commitment: Commitment | null;
  prepState: PrepEntry[];
}

export interface HomeContentProps {
  data: HomeData;
  /** Batch progress 0..1 while the job worker runs (App's model); null
   * when unknown — the Running panel then shows counts only. */
  batchFraction: number | null;
  onNavigate: (view: ViewId, params?: ViewParams) => void;
  /** Open a database game at a ply (Continue card, new-game rows). */
  onOpenGame: (gameId: number, ply: number) => void;
  /** Clock injection for deterministic tests; defaults to now. */
  now?: Date;
}

/** Pure Home renderer — all data via props, no fetching. */
export function HomeContent({ data, batchFraction, onNavigate, onOpenGame, now }: HomeContentProps) {
  const { summary, commitment, prepState } = data;
  const clause = commitmentClause(commitment, prepState);
  const degraded = isDegraded(summary, commitment);
  const [prepQuery, setPrepQuery] = useState("");

  const goPrep = () => {
    onNavigate("prep", prepQuery.trim() ? { opponent: prepQuery.trim() } : undefined);
  };

  const continueCard = summary.lastGame && (
    <div className="home-card accented">
      <div className="home-card-label">CONTINUE</div>
      <div className="home-card-title">
        {summary.lastGame.white} — {summary.lastGame.black}
      </div>
      <div className="home-card-prose">
        Stopped at ply {summary.lastGame.ply} · opened {summary.lastGame.openedAt.slice(0, 10)}.
      </div>
      <button
        className="btn-primary"
        onClick={() => onOpenGame(summary.lastGame!.id, summary.lastGame!.ply)}
      >
        Resume review
      </button>
    </div>
  );

  if (degraded) {
    // The short honest list (maintainer ruling): no invented widgets.
    return (
      <div className="home">
        <div className="home-greeting">
          <h2 className="home-date">{greetingDate(now ?? new Date())}</h2>
        </div>
        {summary.lastGame && (
          <div className="home-cards" style={{ gridTemplateColumns: "1fr 1fr 1fr" }}>
            {continueCard}
          </div>
        )}
        <div className="home-degraded">
          <p>Nothing due today.</p>
          <p>No new games this week.</p>
          <p>
            <button className="home-link" onClick={() => onNavigate("profile")}>
              Build a profile
            </button>{" "}
            to surface findings.
          </p>
        </div>
      </div>
    );
  }

  const prose = findingsProse(summary.findings);
  const since = newSinceLabel(summary);
  const jobs = summary.runningJobs;
  const working = jobs.workerActive || jobs.pending > 0 || jobs.running > 0;

  return (
    <div className="home">
      <div className="home-greeting">
        <h2 className="home-date">{greetingDate(now ?? new Date())}</h2>
        {clause && <span className="home-clause">{clause}</span>}
      </div>

      <div className="home-cards">
        {continueCard}
        <div className="home-card">
          <div className="home-card-label">DUE TODAY</div>
          <div className="home-due-row">
            <span className="home-due-num">{summary.dueSrs}</span>
            <span className="home-due-unit">openings</span>
            {/* Tactics have no honest due count (endless queue): grayed, never a number. */}
            <span
              className="home-due-num muted"
              title="The tactics queue is endless — weakness-weighted selection has no due count."
            >
              –
            </span>
            <span className="home-due-unit">tactics</span>
          </div>
          <div className="home-card-buttons">
            <button className="btn-secondary" onClick={() => onNavigate("train")}>
              Review openings
            </button>
            <button className="btn-secondary" onClick={() => onNavigate("tactics")}>
              Solve tactics
            </button>
          </div>
        </div>
        <div className="home-card">
          <div className="home-card-label">PREP AN OPPONENT</div>
          <div className="home-prep-row">
            <input
              type="text"
              value={prepQuery}
              onChange={(e) => setPrepQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && goPrep()}
              placeholder="Search a name…"
              spellCheck={false}
            />
            <button className="btn-secondary" onClick={goPrep}>
              Go
            </button>
          </div>
          {prepState.length > 0 && (
            <div className="home-recents">
              Recent: {prepState.map((p) => p.opponent).join(" · ")}
            </div>
          )}
        </div>
      </div>

      <div className="home-lower">
        <div className="home-panel home-findings">
          <div className="home-panel-head">
            <span className="home-panel-title">
              {summary.profilePlayer ? `YOUR CHESS · ${summary.profilePlayer.toUpperCase()}` : "YOUR CHESS"}
            </span>
            {summary.profileBuiltAt && (
              <span className="home-pill">BUILT {summary.profileBuiltAt.slice(0, 10)}</span>
            )}
            <div className="home-panel-spacer" />
            <button className="btn-ghost" onClick={() => onNavigate("profile")}>
              Full profile
            </button>
          </div>
          {summary.findingsAvailable ? (
            <>
              {prose && <p className="home-prose">{prose}</p>}
              <div className="home-finding-rows">
                {summary.findings.map((f) => (
                  <button
                    key={f.claimId}
                    className="home-finding"
                    onClick={() => onNavigate("profile", { claim: f.claimId })}
                  >
                    <span className={`role-dot ${findingDotTone(f.claimId)}`} />
                    <span className="home-finding-label">{f.label}</span>
                    <span className={`home-finding-value ${findingDotTone(f.claimId)}`}>
                      {f.value}
                    </span>
                    <span className="home-finding-evidence">
                      {f.evidenceCount} game{f.evidenceCount === 1 ? "" : "s"}
                    </span>
                  </button>
                ))}
              </div>
            </>
          ) : (
            <p className="home-prose dim">
              No profile has been built yet. Build one to surface findings here — static
              analysis plus stored evals, no engine run.
            </p>
          )}
        </div>

        <div className="home-side">
          <div className="home-panel">
            <div className="home-panel-head">
              <span className="home-panel-title">{since ?? "NEW THIS WEEK"}</span>
            </div>
            {summary.newGames.length === 0 ? (
              <p className="home-prose dim">No new games this week.</p>
            ) : (
              <>
                <div className="home-new-rows">
                  {summary.newGames.map((g) => (
                    <button key={g.id} className="home-new-row" onClick={() => onOpenGame(g.id, 0)}>
                      <span className={`source-tag ${sourceTagTone(g.sourceKind, g.source)}`}>
                        {g.source}
                      </span>
                      <span className="home-new-title">
                        {g.white} — {g.black}
                      </span>
                      <span className="home-new-result">{g.result}</span>
                    </button>
                  ))}
                </div>
                <div className="home-new-foot">
                  {summary.newGamesTotal} game{summary.newGamesTotal === 1 ? "" : "s"} this week
                  {summary.newGamesTotal > summary.newGames.length ? " · showing latest" : ""}
                </div>
              </>
            )}
          </div>
          <div className="home-panel">
            <div className="home-panel-head">
              <span className="home-panel-title">RUNNING</span>
            </div>
            {working ? (
              <>
                <div className="home-progress">
                  <span className="home-progress-label">ENGINE JOBS</span>
                  {batchFraction !== null && (
                    <span className="home-progress-track">
                      <span
                        className="home-progress-fill"
                        style={{ width: `${Math.round(batchFraction * 100)}%` }}
                      />
                    </span>
                  )}
                  <span className="home-progress-detail">
                    {batchFraction !== null
                      ? `${Math.round(batchFraction * 100)}%`
                      : `${jobs.pending} pending · ${jobs.running} running`}
                  </span>
                </div>
                <p className="home-prose dim">
                  {jobs.workerActive
                    ? "The job worker is running the queue you started."
                    : "Jobs are queued but the worker is not running — the engine is cold."}
                </p>
              </>
            ) : (
              <p className="home-prose dim">Nothing is running — the engine is cold.</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export interface HomeViewProps {
  /** Bumps when the database (re)opens — triggers a refetch. */
  dbOpen: boolean;
  batchFraction: number | null;
  onNavigate: (view: ViewId, params?: ViewParams) => void;
  onOpenGame: (gameId: number, ply: number) => void;
}

/** Live Home: fetches home_summary + commitment + prep state. */
export default function HomeView({ dbOpen, batchFraction, onNavigate, onOpenGame }: HomeViewProps) {
  const [data, setData] = useState<HomeData | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    if (!dbOpen) return;
    Promise.all([homeSummary(), commitmentGet().catch(() => null), prepStateGet().catch(() => [])])
      .then(([summary, commitment, prepState]) => {
        setData({ summary, commitment, prepState });
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [dbOpen]);

  useEffect(refresh, [refresh]);

  if (!dbOpen || (!data && !error)) {
    return <div className="home"><p className="home-prose dim">Open a database to see your day.</p></div>;
  }
  if (error) {
    return <div className="home"><p className="home-prose dim">Home unavailable: {error}</p></div>;
  }
  return (
    <HomeContent
      data={data!}
      batchFraction={batchFraction}
      onNavigate={onNavigate}
      onOpenGame={onOpenGame}
    />
  );
}
