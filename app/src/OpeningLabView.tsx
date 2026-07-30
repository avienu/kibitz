/**
 * Opening Lab (run 11): diagnose where an opening actually fails the user
 * from their OWN games — book-exit vs middlegame damage, killer
 * structures — then recommend personally-fitting book moves per branch
 * and adopt them into the SRS repertoire.
 *
 * Everything on screen is a static database walk. The engine runs only on
 * two explicit clicks, both through the job queue: "Extend with engine"
 * on a branch (the existing book-extension job) and the ONE cohort
 * re-analysis button (estimate → confirm → enqueue + run). CLAUDE.md #6/#8.
 */
import { useCallback, useEffect, useState } from "react";
import Board from "./Board";
import ScreenHeader from "./shell/ScreenHeader";
import { jobsStatus, matchingPlayers, trainAddLine } from "./lib/db";
import { fmtDurationMs } from "./lib/home";
import {
  evalLabel,
  numberedLine,
  triageExtend,
  triageExtensionStatus,
  type ExtensionStatus,
} from "./lib/triage";
import {
  candidateCoverage,
  cohortCaption,
  coverage,
  fitLabel,
  formatUserCp,
  labCohorts,
  labLineFit,
  labReanalyzeEstimate,
  labReanalyzeStart,
  labReport,
  moveNo,
  statChips,
  unanalyzedNotice,
  verdictText,
  type CohortRow,
  type LabNode,
  type LabReanalyzeEstimate,
  type LabReport,
  type LineFit,
} from "./lib/openingLab";
import type { BoardTreatment } from "./lib/evidence";

const PLAYER_KEY = "kibitz.labPlayer";
const COHORT_KEY = "kibitz.labCohort";

interface OpeningLabViewProps {
  treatment?: BoardTreatment;
  /** Open a database game at a ply (examples + homework deep-link). */
  onOpenGameAt: (gameId: number, ply: number) => void;
  /** Adoption creates SRS cards — let the shell refresh its due badges. */
  onCountsChanged?: () => void;
}

export default function OpeningLabView({
  treatment = "walnut",
  onOpenGameAt,
  onCountsChanged,
}: OpeningLabViewProps) {
  const [player, setPlayer] = useState(() => localStorage.getItem(PLAYER_KEY) ?? "");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [cohorts, setCohorts] = useState<CohortRow[] | null>(null);
  const [cohort, setCohort] = useState<CohortRow | null>(null);
  const [report, setReport] = useState<LabReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [sel, setSel] = useState<LabNode | null>(null);
  const [extStatus, setExtStatus] = useState<ExtensionStatus | null>(null);
  const [extError, setExtError] = useState<string | null>(null);
  const [fits, setFits] = useState<Map<number, LineFit>>(new Map());
  const [adoptMsg, setAdoptMsg] = useState<string | null>(null);
  const [adopting, setAdopting] = useState(false);

  const [reEst, setReEst] = useState<LabReanalyzeEstimate | null>(null);
  const [reRunning, setReRunning] = useState(false);
  const [rePending, setRePending] = useState<number | null>(null);
  const [reError, setReError] = useState<string | null>(null);

  /* ---- player suggestions (debounced) ---- */
  useEffect(() => {
    const q = player.trim();
    if (q.length < 2) return;
    const t = setTimeout(() => {
      matchingPlayers(q)
        .then((names) => setSuggestions(names.slice(0, 8)))
        .catch(() => setSuggestions([]));
    }, 250);
    return () => clearTimeout(t);
  }, [player]);

  /* ---- step 1: the user's openings ---- */
  const loadCohorts = useCallback(async () => {
    const p = player.trim();
    if (p === "" || busy) return;
    setBusy(true);
    setError(null);
    setReport(null);
    setCohort(null);
    setSel(null);
    try {
      const rows = await labCohorts(p);
      setCohorts(rows);
      localStorage.setItem(PLAYER_KEY, p);
      // Reopen the last-picked cohort when it still exists.
      const remembered = localStorage.getItem(COHORT_KEY);
      const match = rows.find((r) => `${r.color}:${r.family}` === remembered);
      if (match) await pickCohortInner(p, match);
    } catch (e) {
      setCohorts(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [player, busy]);

  const pickCohortInner = async (p: string, c: CohortRow) => {
    setCohort(c);
    setSel(null);
    setReEst(null);
    setReError(null);
    setError(null);
    try {
      const r = await labReport(p, c.color, c.ecos);
      setReport(r);
      localStorage.setItem(COHORT_KEY, `${c.color}:${c.family}`);
    } catch (e) {
      setReport(null);
      setError(String(e));
    }
  };

  const pickCohort = useCallback(
    (c: CohortRow) => {
      void pickCohortInner(player.trim(), c);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [player],
  );

  const reloadReport = useCallback(() => {
    if (cohort) void pickCohortInner(player.trim(), cohort);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [player, cohort]);

  /* ---- node selection + extension polling (the triage pattern) ---- */
  const select = useCallback((node: LabNode) => {
    setSel(node);
    setExtStatus(null);
    setExtError(null);
    setFits(new Map());
    setAdoptMsg(null);
  }, []);

  const selFen = sel?.fen ?? null;
  const pollBusy = extStatus?.jobStatus === "pending" || extStatus?.jobStatus === "running";
  useEffect(() => {
    if (!selFen) return;
    let stale = false;
    const fetchStatus = () => {
      triageExtensionStatus(selFen)
        .then((s) => {
          if (!stale) setExtStatus(s);
        })
        .catch((e) => {
          if (!stale) setExtError(String(e));
        });
    };
    fetchStatus();
    if (!pollBusy)
      return () => {
        stale = true;
      };
    const t = setInterval(fetchStatus, 2500);
    return () => {
      stale = true;
      clearInterval(t);
    };
  }, [selFen, pollBusy]);

  // Once candidate lines exist, fetch each line's structure fit (static).
  const extension = extStatus?.extension ?? null;
  useEffect(() => {
    if (!extension || !selFen || extension.fen !== selFen) return;
    let stale = false;
    extension.lines.forEach((line, i) => {
      labLineFit(selFen, line.sans)
        .then((fit) => {
          if (!stale) setFits((m) => new Map(m).set(i, fit));
        })
        .catch(() => {}); // fit is best-effort decoration
    });
    return () => {
      stale = true;
    };
  }, [extension, selFen]);

  const extend = useCallback(async () => {
    if (!selFen) return;
    setExtError(null);
    try {
      await triageExtend(selFen);
      setExtStatus((s) => ({
        extension: s?.extension ?? null,
        jobStatus: "pending",
        jobsAhead: s?.jobsAhead ?? 0,
        workerActive: true,
      }));
    } catch (e) {
      setExtError(String(e));
    }
  }, [selFen]);

  const adopt = useCallback(
    async (sans: string[]) => {
      if (!sel || !cohort || adopting) return;
      setAdopting(true);
      setAdoptMsg(null);
      try {
        const res = await trainAddLine(cohort.color, sans, sel.fen);
        setAdoptMsg(
          `Adopted into "${res.repertoire}": ${res.cardsAdded} new card${
            res.cardsAdded === 1 ? "" : "s"
          }, ${res.cardsExisting} position${res.cardsExisting === 1 ? "" : "s"} already covered.`,
        );
        onCountsChanged?.();
      } catch (e) {
        setAdoptMsg(`Adoption failed: ${e}`);
      } finally {
        setAdopting(false);
      }
    },
    [sel, cohort, adopting, onCountsChanged],
  );

  /* ---- cohort re-analysis: estimate → confirm → enqueue + run ---- */
  const askReanalyze = useCallback(async () => {
    if (!cohort) return;
    setReError(null);
    try {
      setReEst(await labReanalyzeEstimate(player.trim(), cohort.color, cohort.ecos));
    } catch (e) {
      setReError(String(e));
    }
  }, [player, cohort]);

  const startReanalyze = useCallback(async () => {
    if (!cohort) return;
    setReError(null);
    try {
      const started = await labReanalyzeStart(player.trim(), cohort.color, cohort.ecos);
      setReEst(null);
      setReRunning(true);
      setRePending(started.pending);
    } catch (e) {
      setReError(String(e));
    }
  }, [player, cohort]);

  // Inline progress: poll the shared queue while the run is live; when
  // the worker goes idle, rebuild the report with the fresh evals.
  useEffect(() => {
    if (!reRunning) return;
    let stale = false;
    const t = setInterval(() => {
      jobsStatus()
        .then((j) => {
          if (stale) return;
          setRePending(j.pending + j.running);
          if (!j.workerActive) {
            setReRunning(false);
            setRePending(null);
            reloadReport();
          }
        })
        .catch(() => {});
    }, 3000);
    return () => {
      stale = true;
      clearInterval(t);
    };
  }, [reRunning, reloadReport]);

  const notice = report ? unanalyzedNotice(report) : null;
  const orientation = cohort?.color ?? "white";

  return (
    <div className="triage2 lab">
      <ScreenHeader
        title="Opening lab"
        subtitle="Where your opening games actually die — and which book moves fix it"
      />
      <div className="triage-body">
        <div className="triage-main">
          {/* step 1: who + which opening */}
          <div className="triage-search-row">
            <input
              type="text"
              value={player}
              onChange={(e) => setPlayer(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void loadCohorts()}
              placeholder="Your name as it appears in your games…"
              list="lab-player-suggestions"
              spellCheck={false}
            />
            <datalist id="lab-player-suggestions">
              {suggestions.map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
            <button
              className="btn-primary"
              onClick={() => void loadCohorts()}
              disabled={busy || player.trim() === ""}
            >
              {busy ? "Listing your openings…" : "List my openings"}
            </button>
          </div>
          {error && <div className="error">{error}</div>}
          {!cohorts && !error && (
            <p className="triage-footnote">
              Lists the openings you actually play (all your name forms and declared aliases
              count as you), then diagnoses one cohort from your own games: where you leave
              book, whether you are already worse there, and where the first real mistakes
              happen. Static database work — the engine stays off until you explicitly ask.
            </p>
          )}

          {cohorts && (
            <div className="lab-cohorts">
              {cohorts.length === 0 && (
                <div className="pf2-empty">
                  No ECO-tagged decided games for this player yet — import or sync games
                  first.
                </div>
              )}
              {cohorts.map((c) => {
                const key = `${c.color}:${c.family}`;
                const cur = cohort && `${cohort.color}:${cohort.family}` === key;
                return (
                  <button
                    key={key}
                    type="button"
                    className={`triage-row${cur ? " sel" : ""}`}
                    onClick={() => pickCohort(c)}
                  >
                    <span className="triage-row-main">
                      <span className="triage-line">{c.family}</span>
                      <span className="triage-caption">{cohortCaption(c)}</span>
                    </span>
                    <span className="triage-count">{c.games}×</span>
                  </button>
                );
              })}
            </div>
          )}

          {report && cohort && (
            <>
              {/* step 2: the verdict — the product */}
              <div className="lab-verdict">
                <div className="triage-strip-title">
                  THE VERDICT — {cohort.family.toUpperCase()}{" "}
                  {cohort.color === "white" ? "AS WHITE" : "AS BLACK"}
                </div>
                <p className="lab-verdict-text">{verdictText(report)}</p>
                <div className="lab-chips">
                  {statChips(report).map((c) => (
                    <span key={c} className="lab-chip">
                      {c}
                    </span>
                  ))}
                </div>
                {report.structures.some((s) => s.damage > 0) && (
                  <div className="lab-structures">
                    {report.structures
                      .filter((s) => s.damage > 0)
                      .slice(0, 3)
                      .map((s) => (
                        <span key={s.flag} className="lab-chip lab-chip-bad">
                          {s.flag}: {s.scorePct}% over {s.games} game
                          {s.games === 1 ? "" : "s"} · damage {s.damage}
                        </span>
                      ))}
                  </div>
                )}
              </div>

              {/* the ONE honest re-analyze affordance */}
              {notice && (
                <div className="lab-banner">
                  <span>{notice}</span>
                  {reError && <div className="error">{reError}</div>}
                  {reRunning ? (
                    <span className="triage-ext-progress">
                      Re-analyzing through the job queue — {rePending ?? "…"} job
                      {rePending === 1 ? "" : "s"} left…
                    </span>
                  ) : reEst ? (
                    reEst.games === 0 ? (
                      <span className="triage-ext-progress">
                        These games are already queued or analyzed — results land when the
                        job runner finishes.
                      </span>
                    ) : (
                      <span className="lab-confirm">
                        {reEst.games} game{reEst.games === 1 ? "" : "s"}, {reEst.jobs} bounded
                        evals, estimated {fmtDurationMs(reEst.totalEstimateMs)} ·{" "}
                        <em>{reEst.estimateBasis}</em>
                        <button className="btn-primary" onClick={() => void startReanalyze()}>
                          Start now
                        </button>
                        <button className="btn-secondary" onClick={() => setReEst(null)}>
                          Cancel
                        </button>
                      </span>
                    )
                  ) : (
                    <button className="btn-secondary" onClick={() => void askReanalyze()}>
                      Re-analyze {report.unanalyzedGames} game
                      {report.unanalyzedGames === 1 ? "" : "s"}
                    </button>
                  )}
                </div>
              )}

              {/* step 3: branch table */}
              <div className="triage-strip-title">
                BRANCHES — WHERE YOUR MOVES DIVERGE, DAMAGE FIRST
              </div>
              {report.nodes.length === 0 && (
                <div className="triage-none">no in-book branch points found</div>
              )}
              {report.nodes.map((n, i) => (
                <button
                  key={n.fen}
                  type="button"
                  className={`triage-row${sel?.fen === n.fen ? " sel" : ""}`}
                  onClick={() => select(n)}
                >
                  <span className="triage-rank">{String(i + 1).padStart(2, "0")}</span>
                  <span className="triage-row-main">
                    <span className="triage-line">{n.line || "start position"}</span>
                    <span className="triage-caption">
                      move {moveNo(n.ply)} ·{" "}
                      {n.moves
                        .map((m) => `${m.san} ${m.games}× (${m.scorePct}%)`)
                        .join(" · ")}
                      {n.repSan ? ` · book: ${n.repSan}` : ""}
                      {n.hasExtension ? " · engine lines ready" : ""}
                    </span>
                  </span>
                  <span className="triage-count">dmg {n.damage}</span>
                </button>
              ))}

              {/* step 5: structure homework */}
              {report.homework.length > 0 && (
                <>
                  <div className="triage-strip-title">
                    STRUCTURE HOMEWORK — YOUR FIRST ERRORS IN THE KILLER STRUCTURES
                  </div>
                  <div className="triage-examples">
                    {report.homework.map((h) => (
                      <button
                        key={`${h.gameId}-${h.ply}`}
                        type="button"
                        className="triage-example"
                        onClick={() => onOpenGameAt(h.gameId, h.ply)}
                      >
                        #{h.gameId} {h.white} — {h.black}
                        {h.date ? ` · ${h.date}` : ""} · move {moveNo(h.ply)} ·{" "}
                        {formatUserCp(h.beforeCp)} → {formatUserCp(h.afterCp)} ·{" "}
                        {h.structures.join(", ")}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </div>

        {/* step 4: branch detail + recommendation */}
        <aside className="triage-aside">
          <Board
            fen={sel?.fen ?? "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}
            orientation={orientation}
            treatment={treatment}
            size={360}
          />
          {!sel && <div className="triage-aside-caption">SELECT A BRANCH TO SEE THE POSITION</div>}
          {sel && cohort && (
            <>
              <div className="triage-aside-caption">
                {[
                  sel.eco && sel.openingName ? `${sel.eco} ${sel.openingName}` : null,
                  `MOVE ${moveNo(sel.ply)}`,
                  `${sel.games} GAME${sel.games === 1 ? "" : "S"}`,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </div>
              <div className="triage-detail">
                <div className="triage-detail-line">{sel.line || "start position"}</div>
              </div>

              <div className="triage-strip-title">WHAT YOU PLAY HERE</div>
              <div className="lab-moves">
                {sel.moves.map((m) => {
                  const cov = coverage(m);
                  return (
                    <div className="lab-move" key={m.san}>
                      <span className="lab-move-san">{m.san}</span>
                      <span className="lab-move-cells">
                        {m.games}× · {m.scorePct}% ·{" "}
                        {m.avgEvalCp !== null
                          ? `eval ${formatUserCp(m.avgEvalCp)} (${m.evalGames})`
                          : "no evals"}{" "}
                        · {m.inBook ? "book" : "off book"}
                        {m.inRep ? " · in repertoire" : ""}
                        {cov ? ` · replies in book ${cov.pct}% of ${cov.total}` : ""} · dmg{" "}
                        {m.damage}
                      </span>
                    </div>
                  );
                })}
              </div>

              <div className="triage-strip-title">SOURCE GAMES</div>
              <div className="triage-examples">
                {sel.examples.map((ex) => (
                  <button
                    key={`${ex.gameId}-${ex.ply}`}
                    type="button"
                    className="triage-example"
                    onClick={() => onOpenGameAt(ex.gameId, ex.ply)}
                  >
                    #{ex.gameId} {ex.white} — {ex.black}
                    {ex.date ? ` · ${ex.date}` : ""} · played {ex.san} · ply {ex.ply}
                  </button>
                ))}
              </div>

              <div className="triage-extend">
                <div className="triage-strip-title">RECOMMENDATION — PICK A BOOK MOVE</div>
                {extError && <div className="error">{extError}</div>}
                {extension ? (
                  <>
                    <div className="triage-ext-meta">
                      SOUND: {extension.engine} · depth {extension.depth} — FITS: your cached
                      profile — COVERAGE: replies you actually face
                    </div>
                    {!fits.get(0)?.fitAvailable && fits.size > 0 && (
                      <div className="lab-fit-note">
                        No cached profile — build one on the Profile screen to see which
                        lines fit your play.
                      </div>
                    )}
                    {extension.lines.map((line, i) => {
                      const fit = fits.get(i) ?? null;
                      const cov = candidateCoverage(sel, line.sans[0] ?? "");
                      const inRep = sel.repSan !== null && sel.repSan === line.sans[0];
                      return (
                        <div className="triage-ext-line lab-cand" key={i}>
                          <span className="triage-ext-eval">
                            {evalLabel(line, extension.fen)}
                          </span>
                          <span className="triage-ext-sans">
                            {numberedLine(line.sans, extension.fen)}
                            <span className="lab-cand-badges">
                              {inRep && <span className="lab-chip">in repertoire</span>}
                              <span className="lab-chip">
                                {cov
                                  ? `coverage ${cov.pct}% of ${cov.total} replies`
                                  : "coverage: no reply data yet"}
                              </span>
                              {fitLabel(fit) !== null && (
                                <span className="lab-chip">fits: {fitLabel(fit)}</span>
                              )}
                            </span>
                          </span>
                          <button
                            className="btn-secondary"
                            disabled={adopting}
                            onClick={() => void adopt(line.sans)}
                          >
                            Adopt
                          </button>
                        </div>
                      );
                    })}
                    {adoptMsg && <div className="triage-adopt-msg">{adoptMsg}</div>}
                  </>
                ) : extStatus?.jobStatus === "pending" ? (
                  <div className="triage-ext-progress">
                    Queued for the engine
                    {extStatus.jobsAhead > 0
                      ? ` — ${extStatus.jobsAhead} job${
                          extStatus.jobsAhead === 1 ? "" : "s"
                        } ahead of it`
                      : ""}
                    {extStatus.workerActive ? " · worker running…" : " · worker starting…"}
                  </div>
                ) : extStatus?.jobStatus === "running" ? (
                  <div className="triage-ext-progress">
                    Engine analysing — 4 lines, deep search. This can take a few minutes…
                  </div>
                ) : (
                  <>
                    {extStatus?.jobStatus === "failed" && (
                      <div className="error">The last extension attempt failed — retry below.</div>
                    )}
                    <button className="btn-primary" onClick={() => void extend()}>
                      Extend with engine (4 lines)
                    </button>
                    <p className="triage-footnote">
                      Deep MultiPV analysis through the job queue. Clicking is the explicit
                      engine request: the job is queued and the worker starts now.
                    </p>
                  </>
                )}
              </div>
            </>
          )}
        </aside>
      </div>
    </div>
  );
}
