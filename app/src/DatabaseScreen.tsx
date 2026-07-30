/**
 * Database screen (design/handoff-2 §Database): filter chip bar with a
 * right-aligned range readout, the inline job row (the status-strip
 * progress cell promoted into the screen that owns the job), the games
 * table on the shared DataTable, and pagination.
 *
 * Every filter chip is real since run 10: player / ECO / result plus
 * event substring, date range (permissive on PGN "?" wildcards) and
 * source kind. The batch confirm dialog shows the measured-or-assumed
 * `estimateBasis` string verbatim, and states that jobs are resumable.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import ScreenHeader from "./shell/ScreenHeader";
import {
  clearDbScreenState,
  dbScreenState,
  hasActiveFilters,
  updateDbScreenState,
} from "./lib/dbScreenState";
import {
  batchEstimate,
  batchPause,
  batchStart,
  getGame,
  getSavedDbPath,
  listGames,
  createDatabase,
  openDatabase,
  runJobs,
  saveDbPath,
  setWindowTitle,
  type BatchEstimate,
  type BatchKind,
  type DbSummary,
  type GameDetail,
  type GameList,
  type GameRow,
  type JobsStatus,
} from "./lib/db";
import { fmtDurationMs, rangeReadout, sourceTagTone } from "./lib/home";
import { crosstableEligible } from "./lib/crosstable";
import { dateBoundParam, dateRangeHint, SOURCE_KINDS } from "./lib/filters";

const PAGE_SIZE = 50;

/** The spec's exact column template (design/handoff-2 §Database). */
const GRID = "26px 1.6fr 1.6fr 58px 1.2fr 92px 64px 96px 84px";

export interface BatchModel {
  /** 0..1 done fraction of the running batch (App's jobs model). */
  fraction: number;
  /** Jobs in the batch when the worker went active. */
  total: number;
  /** Jobs still pending or running. */
  remaining: number;
}

interface ConfirmState {
  kind: BatchKind;
  estimate: BatchEstimate;
}

/** Batch context the user started this session (labels + time-left). */
interface StartedBatch {
  kind: BatchKind;
  perJobMs: number;
}

interface DatabaseScreenProps {
  summary: DbSummary | null;
  onSummary: (s: DbSummary | null) => void;
  /** Row click — the parent loads the game and switches to the game view. */
  onLoadGame: (detail: GameDetail) => void;
  jobs: JobsStatus | null;
  batch: BatchModel | null;
  onStatus: (msg: string) => void;
  /** Event-cell click — the parent opens the crosstable modal (run 10). */
  onCrosstable: (event: string) => void;
}

function analysisCell(g: GameRow) {
  if (g.analysisKind === "fresh") {
    return <span className="ana-fresh">fresh{g.analysisDepth != null ? ` d${g.analysisDepth}` : ""}</span>;
  }
  if (g.analysisKind === "legacy") return <span className="ana-legacy">legacy</span>;
  return <span className="ana-none">—</span>;
}

export default function DatabaseScreen({
  summary,
  onSummary,
  onLoadGame,
  jobs,
  batch,
  onStatus,
  onCrosstable,
}: DatabaseScreenProps) {
  const [path, setPath] = useState(getSavedDbPath);
  const [opening, setOpening] = useState(false);
  const [dbError, setDbError] = useState<string | null>(null);

  // Filter / page / selection state initializes from the session store
  // (lib/dbScreenState) so navigating away and back restores the search
  // instead of starting from scratch (run-9 field report 1).
  const [playerFilter, setPlayerFilter] = useState(() => dbScreenState().player);
  const [ecoFilter, setEcoFilter] = useState(() => dbScreenState().eco);
  const [resultFilter, setResultFilter] = useState(() => dbScreenState().result);
  const [eventFilter, setEventFilter] = useState(() => dbScreenState().event);
  const [dateMinFilter, setDateMinFilter] = useState(() => dbScreenState().dateMin);
  const [dateMaxFilter, setDateMaxFilter] = useState(() => dbScreenState().dateMax);
  const [sourceKindFilter, setSourceKindFilter] = useState(() => dbScreenState().sourceKind);
  const [page, setPage] = useState(() => dbScreenState().page);
  const [selectedId, setSelectedId] = useState<number | null>(
    () => dbScreenState().selectedGameId,
  );
  const [list, setList] = useState<GameList | null>(null);
  const [listError, setListError] = useState<string | null>(null);

  // Mirror every change back into the store (cheap plain-object writes).
  useEffect(() => {
    updateDbScreenState({
      player: playerFilter,
      eco: ecoFilter,
      result: resultFilter,
      event: eventFilter,
      dateMin: dateMinFilter,
      dateMax: dateMaxFilter,
      sourceKind: sourceKindFilter,
      page,
      selectedGameId: selectedId,
    });
  }, [
    playerFilter,
    ecoFilter,
    resultFilter,
    eventFilter,
    dateMinFilter,
    dateMaxFilter,
    sourceKindFilter,
    page,
    selectedId,
  ]);

  const clearFilters = useCallback(() => {
    clearDbScreenState();
    setPlayerFilter("");
    setEcoFilter("");
    setResultFilter("");
    setEventFilter("");
    setDateMinFilter("");
    setDateMaxFilter("");
    setSourceKindFilter("");
    setPage(0);
    setSelectedId(null);
  }, []);

  // Scroll restore: remember the offset as the user scrolls; put it back
  // once, after the first game list of this mount has rendered rows.
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const scrollRestored = useRef(false);
  useLayoutEffect(() => {
    if (scrollRestored.current || !list) return;
    scrollRestored.current = true;
    if (scrollRef.current) scrollRef.current.scrollTop = dbScreenState().scrollTop;
  }, [list]);

  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [estimating, setEstimating] = useState<BatchKind | null>(null);
  const [pausing, setPausing] = useState(false);
  const startedBatch = useRef<StartedBatch | null>(null);

  const doOpen = useCallback(async () => {
    setOpening(true);
    setDbError(null);
    try {
      const s = await openDatabase(path);
      onSummary(s);
      saveDbPath(path);
      setPage(0);
      const filename = s.path.split(/[\\/]/).pop() ?? s.path;
      setWindowTitle(`kibitz — ${filename}`).catch(() => {});
    } catch (e) {
      onSummary(null);
      setDbError(String(e));
    } finally {
      setOpening(false);
    }
  }, [path, onSummary]);

  const doCreate = useCallback(async () => {
    setOpening(true);
    setDbError(null);
    try {
      const s = await createDatabase();
      onSummary(s);
      saveDbPath(s.path);
      setPage(0);
      const filename = s.path.split(/[\\/]/).pop() ?? s.path;
      setWindowTitle(`kibitz — ${filename}`).catch(() => {});
      onStatus(
        `Created ${s.path} — import PGN (DATA IN / OUT) or link an account sync to fill it.`,
      );
    } catch (e) {
      setDbError(String(e));
    } finally {
      setOpening(false);
    }
  }, [onSummary, onStatus]);

  const [filtering, setFiltering] = useState(false);

  // Game list: refetch on open / filter (debounced) / page change — and
  // whenever the shared summary's game count moves (App re-polls
  // db_summary on one cadence during syncs), so the header count and the
  // list's "1–50 of N" total change at the same moment (audit #8).
  const dbOpen = summary !== null;
  const gamesCount = summary?.games;
  useEffect(() => {
    if (!dbOpen) return;
    let cancelled = false;
    setFiltering(true);
    const t = setTimeout(
      () => {
        listGames(
          {
            playerSubstring: playerFilter || undefined,
            eco: ecoFilter || undefined,
            result: resultFilter || undefined,
            eventSubstring: eventFilter || undefined,
            // Invalid/partial date bounds are held back until they parse
            // (typing "19" must not error every keystroke).
            dateMin: dateBoundParam(dateMinFilter),
            dateMax: dateBoundParam(dateMaxFilter),
            sourceKind: sourceKindFilter || undefined,
          },
          page * PAGE_SIZE,
          PAGE_SIZE,
        )
          .then((l) => {
            if (cancelled) return;
            setList(l);
            setListError(null);
            setFiltering(false);
          })
          .catch((e) => {
            if (cancelled) return;
            setListError(String(e));
            setFiltering(false);
          });
      },
      playerFilter || ecoFilter || eventFilter || dateMinFilter || dateMaxFilter ? 250 : 0,
    );
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
    // gamesCount (not the summary object): identity churns on every
    // shared-cadence refresh; refetch only when the count actually moved.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    dbOpen,
    gamesCount,
    playerFilter,
    ecoFilter,
    resultFilter,
    eventFilter,
    dateMinFilter,
    dateMaxFilter,
    sourceKindFilter,
    page,
  ]);

  const loadById = useCallback(
    async (id: number) => {
      try {
        onLoadGame(await getGame(id));
      } catch (e) {
        setListError(String(e));
      }
    },
    [onLoadGame],
  );

  /* ---- batch operations ---- */

  const askBatch = useCallback(async (kind: BatchKind) => {
    setEstimating(kind);
    try {
      setConfirm({ kind, estimate: await batchEstimate(kind) });
    } catch (e) {
      onStatus(`Estimate failed: ${e}`);
    } finally {
      setEstimating(null);
    }
  }, [onStatus]);

  const startBatch = useCallback(async () => {
    if (!confirm) return;
    const { kind, estimate } = confirm;
    setConfirm(null);
    try {
      const started = await batchStart(kind);
      if (started.jobsEnqueued > 0) {
        startedBatch.current = {
          kind,
          perJobMs: estimate.totalEstimateMs / started.jobsEnqueued,
        };
      }
      onStatus(
        `${kind === "annotate" ? "Annotate" : "Fresh analysis"}: ${started.gamesEnqueued} game(s), ` +
          `${started.jobsEnqueued} job(s) enqueued (already-covered games skipped).`,
      );
      // Enqueueing is passive; the worker is the user-initiated engine
      // entry point. Start it now — that is what the user just confirmed.
      await runJobs();
    } catch (e) {
      onStatus(`Batch start: ${e}`);
    }
  }, [confirm, onStatus]);

  const doPause = useCallback(async () => {
    setPausing(true);
    try {
      const was = await batchPause();
      onStatus(
        was
          ? "Pausing between jobs — everything unstarted stays pending; run again to resume."
          : "Nothing was running.",
      );
    } catch (e) {
      onStatus(`Pause failed: ${e}`);
    } finally {
      setPausing(false);
    }
  }, [onStatus]);

  /* ---- inline job row ---- */
  const batchWorking = (jobs?.workerActive ?? false) || (jobs != null && jobs.pending > 0);
  const jobLabel =
    startedBatch.current?.kind === "annotate"
      ? "ANNOTATING DATABASE"
      : startedBatch.current?.kind === "fresh-analysis"
        ? "FRESH ANALYSIS PASS"
        : "ENGINE JOBS";
  let jobDetail = "";
  if (batch) {
    const done = batch.total - batch.remaining;
    jobDetail = `${Math.round(batch.fraction * 100)}% · ${done.toLocaleString("en-US")} / ${batch.total.toLocaleString("en-US")}`;
    if (startedBatch.current) {
      jobDetail += ` · ${fmtDurationMs(batch.remaining * startedBatch.current.perJobMs)} left`;
    }
  } else if (jobs) {
    jobDetail = `${jobs.pending.toLocaleString("en-US")} pending · ${jobs.running} running`;
  }

  const columns: DataTableColumn<GameRow>[] = [
    {
      key: "dup",
      header: "",
      render: (g) => (g.dup ? <span className="dup-flag" title="duplicate copies linked">⑂</span> : null),
    },
    { key: "white", header: "WHITE", render: (g) => g.white, sort: (a, b) => a.white.localeCompare(b.white) },
    { key: "black", header: "BLACK", render: (g) => g.black, sort: (a, b) => a.black.localeCompare(b.black) },
    { key: "result", header: "RES", render: (g) => <span className="cell-result">{g.result}</span> },
    {
      key: "event",
      header: "EVENT",
      render: (g) =>
        crosstableEligible(g.event) ? (
          <button
            className="cell-dim cell-event-link"
            title="Open the crosstable for this event"
            onClick={(e) => {
              e.stopPropagation();
              onCrosstable(g.event);
            }}
          >
            {g.event}
          </button>
        ) : (
          <span className="cell-dim">{g.event}</span>
        ),
    },
    {
      key: "date",
      header: "DATE",
      render: (g) => <span className="cell-date">{g.date ?? ""}</span>,
      sort: (a, b) => (a.date ?? "").localeCompare(b.date ?? ""),
    },
    {
      key: "eco",
      header: "ECO",
      render: (g) => <span className="cell-eco">{g.eco ?? ""}</span>,
      sort: (a, b) => (a.eco ?? "").localeCompare(b.eco ?? ""),
    },
    {
      key: "source",
      header: "SOURCE",
      render: (g) => (
        <span className={`source-tag ${sourceTagTone(g.sourceKind, g.source)}`}>{g.source}</span>
      ),
    },
    { key: "analysis", header: "ANALYSIS", render: analysisCell },
  ];

  const totalPages = list ? Math.max(1, Math.ceil(list.total / PAGE_SIZE)) : 1;
  const filename = summary ? (summary.path.split(/[\\/]/).pop() ?? summary.path) : null;

  return (
    <>
      <ScreenHeader
        title="Database"
        subtitle={
          summary
            ? `${filename} · ${summary.games.toLocaleString("en-US")} games · personal > TWIC > online`
            : "no database open"
        }
        actions={
          summary && (
            <>
              <button
                className="btn-secondary"
                disabled={estimating !== null}
                onClick={() => void askBatch("annotate")}
              >
                {estimating === "annotate" ? "Estimating…" : "Annotate database"}
              </button>
              <button
                className="btn-secondary"
                disabled={estimating !== null}
                onClick={() => void askBatch("fresh-analysis")}
              >
                {estimating === "fresh-analysis" ? "Estimating…" : "Fresh analysis pass"}
              </button>
            </>
          )
        }
      />
      <div
        className="page-scroll"
        ref={scrollRef}
        onScroll={(e) => updateDbScreenState({ scrollTop: e.currentTarget.scrollTop })}
      >
        <div className="dbscreen">
          {!summary && (
            <div className="db-open-row">
              <input
                type="text"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="path to .sqlite database"
                spellCheck={false}
              />
              <button onClick={() => void doOpen()} disabled={opening}>
                {opening ? "Opening…" : "Open"}
              </button>
              {/* First-run path (2026-07-30, tester builds): a fresh
                * install has nothing to browse — open_database refuses
                * missing paths by design, so creating is its own
                * explicit action. */}
              <button
                className="btn-secondary"
                disabled={opening}
                onClick={() => void doCreate()}
                title="Create an empty database in the app's own storage, then import PGN or sync games into it"
              >
                Create new database
              </button>
            </div>
          )}
          {dbError && <div className="error">{dbError}</div>}

          {summary && (
            <>
              <div className="filter-bar">
                <input
                  className="filter-chip-input"
                  type="text"
                  value={playerFilter}
                  onChange={(e) => {
                    setPlayerFilter(e.target.value);
                    setPage(0);
                  }}
                  placeholder="Player — type to filter"
                  spellCheck={false}
                />
                <input
                  className="filter-chip-input"
                  type="text"
                  value={eventFilter}
                  onChange={(e) => {
                    setEventFilter(e.target.value);
                    setPage(0);
                  }}
                  placeholder="Event"
                  spellCheck={false}
                />
                <input
                  className="filter-chip-input date"
                  type="text"
                  value={dateMinFilter}
                  onChange={(e) => {
                    setDateMinFilter(e.target.value);
                    setPage(0);
                  }}
                  placeholder="From YYYY[.MM[.DD]]"
                  title="Inclusive lower date bound. PGN wildcard dates (1992.??.??) match any range they could fall in; undated games are excluded."
                  spellCheck={false}
                />
                <input
                  className="filter-chip-input date"
                  type="text"
                  value={dateMaxFilter}
                  onChange={(e) => {
                    setDateMaxFilter(e.target.value);
                    setPage(0);
                  }}
                  placeholder="To YYYY[.MM[.DD]]"
                  title="Inclusive upper date bound"
                  spellCheck={false}
                />
                <input
                  className="filter-chip-input eco"
                  type="text"
                  value={ecoFilter}
                  onChange={(e) => {
                    setEcoFilter(e.target.value.toUpperCase());
                    setPage(0);
                  }}
                  placeholder="ECO"
                  maxLength={3}
                  spellCheck={false}
                />
                <select
                  className="filter-chip-select"
                  value={resultFilter}
                  onChange={(e) => {
                    setResultFilter(e.target.value);
                    setPage(0);
                  }}
                >
                  <option value="">Result</option>
                  <option value="1-0">1-0</option>
                  <option value="0-1">0-1</option>
                  <option value="1/2-1/2">½-½</option>
                  <option value="*">*</option>
                </select>
                <select
                  className="filter-chip-select"
                  value={sourceKindFilter}
                  onChange={(e) => {
                    setSourceKindFilter(e.target.value);
                    setPage(0);
                  }}
                  title="Provenance kind of the game's source"
                >
                  <option value="">Source</option>
                  {SOURCE_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
                {hasActiveFilters({
                  player: playerFilter,
                  eco: ecoFilter,
                  result: resultFilter,
                  event: eventFilter,
                  dateMin: dateMinFilter,
                  dateMax: dateMaxFilter,
                  sourceKind: sourceKindFilter,
                  page,
                  scrollTop: 0,
                  selectedGameId: null,
                }) && (
                  <button
                    className="filter-chip-clear"
                    onClick={clearFilters}
                    title="Reset filters, page and selection"
                  >
                    Clear
                  </button>
                )}
                <div className="filter-spacer" />
                {list && (
                  <span className="range-readout">
                    {filtering
                      ? "filtering…"
                      : rangeReadout(page * PAGE_SIZE, list.rows.length, list.total)}
                  </span>
                )}
              </div>
              {dateRangeHint(dateMinFilter, dateMaxFilter) && (
                <div className="filter-hint">{dateRangeHint(dateMinFilter, dateMaxFilter)}</div>
              )}

              {batchWorking && (
                <div className="inline-job-row">
                  <span className="inline-job-label">{jobLabel}</span>
                  <span className="inline-job-track">
                    <span
                      className="inline-job-fill"
                      style={{ width: `${Math.round((batch?.fraction ?? 0) * 100)}%` }}
                    />
                  </span>
                  <span className="inline-job-detail">{jobDetail}</span>
                  <button className="btn-ghost" onClick={() => void doPause()} disabled={pausing}>
                    {pausing ? "Pausing…" : "Pause"}
                  </button>
                </div>
              )}

              {listError && <div className="error">{listError}</div>}
              {!list && !listError && (
                // First list query still in flight (it can queue behind a
                // bulk sync for seconds — audit #7): show skeleton rows,
                // never a bare void.
                <div className="db-skeleton" role="status" aria-label="Loading games">
                  {Array.from({ length: 8 }, (_, i) => (
                    <div key={i} className="db-skeleton-row" />
                  ))}
                  <div className="db-skeleton-note">Loading games…</div>
                </div>
              )}
              {list && (
                <DataTable
                  columns={columns}
                  rows={list.rows}
                  gridTemplate={GRID}
                  rowKey={(g) => g.id}
                  onRowClick={(g) => {
                    setSelectedId(g.id);
                    void loadById(g.id);
                  }}
                  rowClassName={(g) => (g.id === selectedId ? "row-selected" : undefined)}
                  empty="No games match the filters."
                  footer={
                    <div className="pager-row">
                      <button
                        className="btn-ghost"
                        onClick={() => setPage((p) => Math.max(0, p - 1))}
                        disabled={page === 0}
                      >
                        ◀
                      </button>
                      <span className="pager-readout">
                        page {(page + 1).toLocaleString("en-US")} of {totalPages.toLocaleString("en-US")}
                      </span>
                      <button
                        className="btn-ghost"
                        onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                        disabled={page + 1 >= totalPages}
                      >
                        ▶
                      </button>
                      <div className="filter-spacer" />
                      <span className="pager-note">
                        ⑂ marks a duplicate — linked to its higher-priority copy, never deleted.
                        Source priority: personal &gt; TWIC &gt; online.
                      </span>
                    </div>
                  }
                />
              )}
            </>
          )}
        </div>
      </div>

      {confirm && (
        <div className="modal-overlay" onClick={() => setConfirm(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-title">
              {confirm.kind === "annotate" ? "Annotate database" : "Fresh analysis pass"}
            </div>
            <p className="modal-prose">
              {confirm.estimate.games.toLocaleString("en-US")} game
              {confirm.estimate.games === 1 ? "" : "s"} to cover
              {confirm.estimate.games > 0 &&
                ` · estimated ${fmtDurationMs(confirm.estimate.totalEstimateMs)}`}
              . Jobs are resumable — pause anytime and the run picks up exactly where it left
              off; games already covered are skipped.
            </p>
            <p className="modal-basis">Estimate basis: {confirm.estimate.estimateBasis}</p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button
                className="btn-primary"
                onClick={() => void startBatch()}
                disabled={confirm.estimate.games === 0}
              >
                {confirm.estimate.games === 0 ? "Nothing to do" : "Start"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
