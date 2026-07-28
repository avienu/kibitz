/**
 * The game view (design/handoff-1 §Screen 2): header bar → board column
 * (eval bar + walnut board + move controls) → right pane (Explain over
 * Moves). Owns the prose⇄board linkage wiring; all derivations live in
 * lib/gameView.ts.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import Board, { type BoardMovable } from "./Board";
import EvalBar from "./EvalBar";
import ExplainPanel from "./ExplainPanel";
import MovesPanel, { type MovesEditing } from "./MovesPanel";
import { evalsByPly, type AnalysisRow } from "./lib/analyses";
import {
  annotateGame,
  batchPause,
  exportGamePgn,
  jobsStatus,
  reanalyzeGame,
  runJobs,
  type AnnotateSummary,
  type JobsStatus,
} from "./lib/db";
import { repGlyphsByPly, type RepertoireMark } from "./lib/repMarks";
import { analyzeLive, getSavedEnginePath, onEngineInfo, stopAnalysis } from "./lib/engine";
import { formatScore, summarizeInfo, type EngineInfo } from "./lib/engineView";
import { boardGeometry } from "./lib/evidence";
import { liveInitial, liveReduce, type LiveEvent } from "./lib/liveAnalysis";
import { numberSanLine, uciPvToSan, PV_DISPLAY_PLIES, PV_INSERT_PLIES } from "./lib/pv";
import type { VariationPreview } from "./lib/preview";
import {
  COACH_HOVER_INDEX,
  deriveEvidence,
  deriveIntensity,
  fitBoardSize,
  selectPlyAnalysis,
  type ExplanationJson,
  type GameViewAction,
  type GameViewState,
} from "./lib/gameView";
import type { LoadedGame } from "./lib/game";
import { gameEngines, movesRows, movesRowsFromSans, type MovesRow } from "./lib/movesView";
import { buildAnnView, insertVariation } from "./lib/tokens";
import { crosstableEligible } from "./lib/crosstable";

interface PendingVariation {
  ply: number;
  san: string;
  label: string;
}

interface GameViewProps {
  game: LoadedGame | null;
  fen: string;
  lastMove?: [string, string];
  movable?: BoardMovable;
  /** Promotion picker overlay element (rendered over the board). */
  promoElement: React.ReactNode;
  gv: GameViewState;
  dispatch: (a: GameViewAction) => void;
  plyCount: number;
  /** Database-game context (annotation editing + header actions). */
  gameId: number | null;
  editing: MovesEditing | null;
  analysisRows: AnalysisRow[];
  /** Repertoire marks for the loaded game (empty = no repertoire). */
  repMarks: RepertoireMark[];
  explainOn: boolean;
  explanation: ExplanationJson | null;
  explaining: boolean;
  explainedPlies: number[];
  onExplain: () => void;
  pendingVar: PendingVariation | null;
  onAcceptVar: () => void;
  onDismissVar: () => void;
  onAddToRepertoire: (color: "white" | "black") => void;
  onReload: () => void;
  onStatus: (s: string) => void;
  /** Variation preview (run-9 round 2): non-null while previewing. */
  preview: VariationPreview | null;
  onPreviewVariation: (row: Extract<MovesRow, { kind: "variation" }>) => void;
  onPreviewStep: (delta: number) => void;
  onExitPreview: () => void;
  /** Event-line click — the parent opens the crosstable modal (run 10). */
  onCrosstable: (event: string) => void;
}

/** Percent position of a square in the grid for an orientation. */
function squarePos(square: string, flipped: boolean): CSSProperties {
  const f = square.charCodeAt(0) - 97;
  const r = square.charCodeAt(1) - 49;
  const x = flipped ? 7 - f : f;
  const y = flipped ? r : 7 - r;
  return { left: `${x * 12.5}%`, top: `${y * 12.5}%` };
}

/** Square name from a click position inside the grid overlay. */
function squareAt(xFrac: number, yFrac: number, flipped: boolean): string | null {
  if (xFrac < 0 || xFrac >= 1 || yFrac < 0 || yFrac >= 1) return null;
  let file = Math.floor(xFrac * 8);
  let rank = 7 - Math.floor(yFrac * 8);
  if (flipped) {
    file = 7 - file;
    rank = 7 - rank;
  }
  return `${String.fromCharCode(97 + file)}${rank + 1}`;
}

export default function GameView({
  game,
  fen,
  lastMove,
  movable,
  promoElement,
  gv,
  dispatch,
  plyCount,
  gameId,
  editing,
  analysisRows,
  repMarks,
  explainOn,
  explanation,
  explaining,
  explainedPlies,
  onExplain,
  pendingVar,
  onAcceptVar,
  onDismissVar,
  onAddToRepertoire,
  onReload,
  onStatus,
  preview,
  onPreviewVariation,
  onPreviewStep,
  onExitPreview,
  onCrosstable,
}: GameViewProps) {
  const colRef = useRef<HTMLDivElement | null>(null);
  const [boardSize, setBoardSize] = useState(656);

  /* ---- live analysis (run-8): explicit toggle, go-infinite, hard stop ---- */
  const [live, setLive] = useState(liveInitial);
  // Each info payload arrives stamped (by the backend) with the FEN the
  // search was STARTED on, so a stale PV/eval from the previous position
  // is never rendered against the new one — it would flip the score's
  // sign (audit 2026-07 #5) and produce an illegal SAN line. Stamping
  // client-side with "the fen we are currently searching" is NOT enough:
  // infos from a just-stopped search keep streaming after a restart.
  const [liveInfo, setLiveInfo] = useState<{ info: EngineInfo; fen: string } | null>(null);
  const liveRef = useRef(live);
  liveRef.current = live;
  const liveDispatch = useCallback((event: LiveEvent) => {
    const { next, commands } = liveReduce(liveRef.current, event);
    liveRef.current = next;
    setLive(next);
    for (const c of commands) {
      if (c.kind === "stop") void stopAnalysis().catch(() => {});
      else void analyzeLive(c.fen, getSavedEnginePath()).catch(() => {});
    }
    if (!next.on) setLiveInfo(null);
  }, []);
  useEffect(() => {
    // Follow the shown position while live; no-op when off.
    liveDispatch({ type: "fenChanged", fen });
  }, [fen, liveDispatch]);
  useEffect(() => {
    // Streamed PV/eval while live only, attributed to the fen the search
    // actually ran on (the event's own stamp — see the note above).
    let un: (() => void) | undefined;
    onEngineInfo((info) => {
      if (liveRef.current.on && info.fen) {
        setLiveInfo({ info, fen: info.fen });
      }
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);
  useEffect(
    () => () => {
      // Unmount (view change / game close) hard-stops the search.
      liveDispatch({ type: "leave" });
    },
    [liveDispatch],
  );

  // Info for the SHOWN position only (see the stamped-fen note above).
  const liveCur = liveInfo && liveInfo.fen === fen ? liveInfo.info : null;
  const pvSans = useMemo(() => (liveCur?.pv ? uciPvToSan(fen, liveCur.pv) : []), [liveCur, fen]);
  const pvLineShort = useMemo(
    () => (pvSans.length > 0 ? numberSanLine(fen, pvSans, PV_DISPLAY_PLIES) : ""),
    [fen, pvSans],
  );
  const pvLineFull = useMemo(
    () => (pvSans.length > 0 ? numberSanLine(fen, pvSans) : ""),
    [fen, pvSans],
  );
  // "Add as variation": a db game is loaded (editing exists), the PV is
  // non-empty, and there is a mainline move at the current ply to vary.
  // Disabled while previewing a variation (the PV belongs to the preview
  // position, not to any mainline ply).
  const canAddPv =
    editing !== null && pvSans.length > 0 && gv.ply < plyCount && preview === null;
  const addPvAsVariation = () => {
    if (!editing || !liveCur || !canAddPv) return;
    const tag = `ENGINE${liveCur.depth !== undefined ? ` d${liveCur.depth}` : ""} ${formatScore(liveCur, fen)}`;
    editing.onChange(
      insertVariation(editing.tokens, gv.ply + 1, pvSans.slice(0, PV_INSERT_PLIES), tag),
    );
    onStatus(
      `Engine line added as a variation of ${game?.sans[gv.ply] ?? "the next move"} — Save to keep it.`,
    );
  };

  const [exportText, setExportText] = useState<string | null>(null);
  const [acting, setActing] = useState(false);

  /* ---- run-9: Re-analyze / Annotate run immediately on click ----
   * An explicit click IS an explicit engine request — the engine-off
   * principle (CLAUDE.md #6) governs defaults, not user actions. The
   * click enqueues AND starts the jobs worker; this inline row shows the
   * batch progressing right here in the game view. */
  interface GameBatch {
    kind: "reanalyze" | "annotate";
    /** Jobs this click enqueued (the label's honest count). */
    positions: number;
    /** pending+running snapshot once the worker was started — the
     * progress base (covers this enqueue plus anything already queued). */
    total: number;
    done: boolean;
  }
  const [gameBatch, setGameBatch] = useState<GameBatch | null>(null);
  const [batchJobs, setBatchJobs] = useState<JobsStatus | null>(null);
  const [pausing, setPausing] = useState(false);

  /** Enqueue happened — start the worker (or join the active one) and
   * snapshot the progress base. Returns true when a worker was already
   * running (our jobs joined its queue). */
  const startGameBatch = async (kind: GameBatch["kind"], positions: number) => {
    let joined = false;
    try {
      await runJobs();
    } catch {
      joined = true;
    }
    const j = await jobsStatus().catch(() => null);
    const total = Math.max(positions, (j?.pending ?? 0) + (j?.running ?? 0));
    setBatchJobs(j);
    setGameBatch({ kind, positions, total, done: false });
    return joined;
  };

  // Poll jobs_status at 1s while our batch runs — the inline row's counts.
  // (App's 3s shell poll stays untouched; this one is scoped to the batch
  // and stops the moment the row reaches its done-state.)
  useEffect(() => {
    if (!gameBatch || gameBatch.done) return;
    let cancelled = false;
    const tick = () => {
      jobsStatus()
        .then((j) => {
          if (!cancelled) setBatchJobs(j);
        })
        .catch(() => {});
    };
    tick();
    const t = setInterval(tick, 1000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [gameBatch]);

  // Batch completion: worker idle with nothing left → done-state row.
  // App also reloads on worker-idle; onReload here covers a run that
  // finishes between App's 3s polls, so results always appear.
  useEffect(() => {
    if (!gameBatch || gameBatch.done || !batchJobs) return;
    if (!batchJobs.workerActive && batchJobs.pending + batchJobs.running === 0) {
      setGameBatch({ ...gameBatch, done: true });
      onReload();
    }
  }, [batchJobs, gameBatch, onReload]);
  useEffect(() => {
    if (!gameBatch?.done) return;
    const t = setTimeout(() => setGameBatch(null), 6000);
    return () => clearTimeout(t);
  }, [gameBatch]);
  // A different game loaded: the row's context is gone.
  useEffect(() => setGameBatch(null), [gameId]);

  const batchRemaining = batchJobs ? batchJobs.pending + batchJobs.running : null;
  const batchFraction =
    gameBatch === null
      ? 0
      : gameBatch.done || gameBatch.total === 0
        ? 1
        : Math.max(0, Math.min(1, 1 - (batchRemaining ?? gameBatch.total) / gameBatch.total));

  const doPauseBatch = async () => {
    setPausing(true);
    try {
      const was = await batchPause();
      onStatus(
        was
          ? "Pausing between jobs — everything unstarted stays pending; Re-analyze or Jobs resumes."
          : "Nothing was running.",
      );
    } catch (e) {
      onStatus(`Pause failed: ${e}`);
    } finally {
      setPausing(false);
    }
  };

  // Resize (deliverable 2c): the board column absorbs extra width; the
  // board snaps to the largest multiple of 8 that fits (min 496).
  useLayoutEffect(() => {
    const el = colRef.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      // Horizontal: column padding (26×2) + eval bar (~46px incl. gap).
      // Vertical: padding (22×2) + gaps (18×2) + move-controls row (~40px).
      setBoardSize(fitBoardSize(r.width - 52 - 46, r.height - 120, gv.boardTreatment));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [gv.boardTreatment]);

  // Alert collapse (audit #13): the panel shows the top 3 alert
  // sentences by default; "show N more" reveals the rest. Owned here so
  // the board's no-hover union tracks what is actually visible. Resets
  // whenever the explanation (i.e. the ply) changes.
  const [alertsExpanded, setAlertsExpanded] = useState(false);
  useEffect(() => setAlertsExpanded(false), [explanation]);

  const evidence = useMemo(
    () =>
      explainOn
        ? deriveEvidence(explanation, gv.hoverSentence, {
            previewing: preview !== null,
            expandedAlerts: alertsExpanded,
          })
        : null,
    [explainOn, explanation, gv.hoverSentence, preview, alertsExpanded],
  );
  const intensity = deriveIntensity(gv.hoverSentence);

  const geo = boardGeometry(boardSize, gv.boardTreatment);
  const gridOffset = { top: geo.framePad, left: geo.framePad + geo.gutter };

  // Square click → selection (ring + prose filter). The overlay covers
  // exactly the grid; clicks bubble up from chessground untouched.
  const gridOverlayRef = useRef<HTMLDivElement | null>(null);
  const onBoardClick = useCallback(
    (e: React.MouseEvent) => {
      const el = gridOverlayRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const sq = squareAt((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height, gv.flipped);
      if (sq) dispatch({ type: "selectSquare", square: sq });
    },
    [dispatch, gv.flipped],
  );

  const rows: MovesRow[] = useMemo(() => {
    if (editing && game) {
      const view = buildAnnView(game.fens[0], editing.tokens);
      if (!view.error || view.items.length > 0) {
        return movesRows(view, game.fens[0], gameEngines(analysisRows), editing.narrations);
      }
    }
    return game ? movesRowsFromSans(game.sans, game.fens[0]) : [];
  }, [editing, game, analysisRows]);

  const evalsMap = useMemo(() => evalsByPly(analysisRows), [analysisRows]);
  // Marks only render when repertoires exist — no toggle needed (run-9).
  const repGlyphs = useMemo(
    () => (repMarks.length > 0 ? repGlyphsByPly(repMarks) : null),
    [repMarks],
  );
  const evalRow = useMemo(() => selectPlyAnalysis(analysisRows, gv.ply), [analysisRows, gv.ply]);
  // Mate distance for the eval bar, when the current explanation knows it.
  const mate = explanation?.eval?.mate ?? null;

  const step = (delta: number) => dispatch({ type: "step", delta, plyCount });
  const setPly = (ply: number) => dispatch({ type: "setPly", ply, plyCount });

  const headers = game?.headers ?? {};
  const title = game ? `${headers["White"] ?? "?"} — ${headers["Black"] ?? "?"}` : "No game loaded";
  const meta = game
    ? [
        [headers["Site"], headers["Date"]?.slice(0, 4)].filter(Boolean).join(", "),
        // "Philidor Defence, C41" when the db resolved the opening name.
        [headers["Opening"], headers["ECO"]].filter(Boolean).join(", "),
        `${game.sans.length} plies`,
        gameId !== null ? `database #${gameId}` : "pasted PGN",
      ]
        .filter(Boolean)
        .join(" · ")
    : "Open a game from the Database, or paste a PGN under Import.";

  const doAnnotate = async () => {
    if (gameId === null) return;
    setActing(true);
    try {
      const s: AnnotateSummary = await annotateGame(gameId);
      onReload();
      if (s.jobsEnqueued > 0) {
        // Run the confirm jobs now — the click asked for them (run-9).
        const joined = await startGameBatch("annotate", s.jobsEnqueued);
        onStatus(
          `Annotated: ${s.positionsAnalyzed} positions, ${s.screensFired} screens fired, ` +
            `${s.commentsAdded} comments — ${s.jobsEnqueued} engine confirmation(s) ` +
            (joined ? "added to the already-running worker." : "running now."),
        );
      } else {
        onStatus(
          s.screensFired > 0
            ? `Narrations regenerated (${s.commentsAdded} comments over ` +
              `${s.positionsAnalyzed} positions) — all ${s.screensFired} tactical ` +
              `screens were already engine-verified, so nothing changed visibly.`
            : `Annotated: ${s.positionsAnalyzed} positions, ${s.commentsAdded} comments — ` +
              `no tactical screens fired, nothing for the engine to confirm.`,
        );
      }
    } catch (e) {
      onStatus(`Annotate failed: ${e}`);
    } finally {
      setActing(false);
    }
  };
  const doReanalyze = async () => {
    if (gameId === null) return;
    setActing(true);
    try {
      const n = await reanalyzeGame(gameId);
      if (n === 0) {
        onStatus("Nothing to enqueue — every position is already queued or freshly analyzed.");
        return;
      }
      const joined = await startGameBatch("reanalyze", n);
      onStatus(
        `${n} position(s) ` +
          (joined
            ? "added to the already-running worker — progress above."
            : "queued and running now — evals and annotations refresh when it finishes."),
      );
    } catch (e) {
      onStatus(`Re-analyze failed: ${e}`);
    } finally {
      setActing(false);
    }
  };
  const doExport = async () => {
    if (gameId === null) return;
    try {
      setExportText(await exportGamePgn(gameId));
    } catch (e) {
      onStatus(`Export failed: ${e}`);
    }
  };

  return (
    <div className="game-view">
      <header className="header-bar">
        <div className="header-title-block">
          <div className="header-title-row">
            <span className="header-title">{title}</span>
            {game && <span className="header-result">{headers["Result"] ?? ""}</span>}
          </div>
          <div className="header-meta">{meta}</div>
          {game && crosstableEligible(headers["Event"]) && (
            <button
              className="header-event-link"
              title="Open the crosstable for this event"
              onClick={() => onCrosstable(headers["Event"]!)}
            >
              {headers["Event"]} · crosstable
            </button>
          )}
        </div>
        <div className="header-actions">
          <span className="seg" role="group" aria-label="Board treatment">
            {(["walnut", "instrument"] as const).map((t) => (
              <button
                key={t}
                className={gv.boardTreatment === t ? "cur" : ""}
                onClick={() => dispatch({ type: "setTreatment", treatment: t })}
              >
                {t}
              </button>
            ))}
          </span>
          <span className="header-divider" />
          <button
            className="btn"
            onClick={() => void doAnnotate()}
            disabled={gameId === null || acting}
            title="The coach: write prose comments for the whole game (static, free) and engine-verify any tactical alerts"
          >
            Annotate
          </button>
          <button
            className="btn"
            onClick={() => void doReanalyze()}
            disabled={gameId === null || acting}
            title="The numbers: fresh engine evaluation of every position — fills the eval column and eval bar"
          >
            Re-analyze
          </button>
          <button className="btn" onClick={() => void doExport()} disabled={gameId === null}>
            Export PGN
          </button>
        </div>
      </header>

      {gameBatch && (
        <div className="inline-job-row game-inline-job" role="status">
          <span className="inline-job-label">
            {gameBatch.kind === "reanalyze" ? "REANALYZING" : "ANNOTATING"} ·{" "}
            {gameBatch.positions.toLocaleString("en-US")} position
            {gameBatch.positions === 1 ? "" : "s"}
          </span>
          <span className="inline-job-track">
            <span className="inline-job-fill" style={{ width: `${Math.round(batchFraction * 100)}%` }} />
          </span>
          <span className="inline-job-detail">
            {gameBatch.done
              ? "done — evals and annotations refreshed"
              : `${Math.round(batchFraction * 100)}% · ${Math.max(
                  0,
                  gameBatch.total - (batchRemaining ?? gameBatch.total),
                ).toLocaleString("en-US")} / ${gameBatch.total.toLocaleString("en-US")} jobs`}
          </span>
          {gameBatch.done ? (
            <button className="btn-ghost" onClick={() => setGameBatch(null)} title="Dismiss">
              ✕
            </button>
          ) : (
            <button className="btn-ghost" onClick={() => void doPauseBatch()} disabled={pausing}>
              {pausing ? "Pausing…" : "Pause"}
            </button>
          )}
        </div>
      )}

      <div className="game-main">
        <div className="board-column" ref={colRef}>
          <div className="board-row">
            <EvalBar
              row={preview ? null : evalRow}
              mate={preview ? null : mate}
              height={boardSize}
            />
            <div className="board-wrap" onClick={onBoardClick}>
              <Board
                fen={fen}
                lastMove={lastMove}
                movable={movable}
                orientation={gv.flipped ? "black" : "white"}
                treatment={gv.boardTreatment}
                size={boardSize}
                evidence={evidence}
                intensity={intensity}
              />
              {/* Selection ring overlay — aligned to the grid, above it. */}
              <div
                ref={gridOverlayRef}
                className="board-sel-overlay"
                style={
                  {
                    top: gridOffset.top,
                    left: gridOffset.left,
                    width: geo.size,
                    height: geo.size,
                    "--sb-cell": `${geo.cell}px`,
                  } as CSSProperties
                }
                aria-hidden
              >
                {gv.selectedSquare && !preview && (
                  <div
                    className="kibitz-mark kibitz-mark-selected"
                    style={squarePos(gv.selectedSquare, gv.flipped)}
                  />
                )}
              </div>
              {promoElement}
            </div>
          </div>

          {preview && (
            <div className="preview-pill" role="status">
              <span className="preview-tag">PREVIEWING VARIATION</span>
              <span className="preview-label">{preview.label}</span>
              <span className="btn-group preview-nav">
                <button
                  onClick={() => onPreviewStep(-1)}
                  disabled={preview.at === 0}
                  title="Back within the variation (←)"
                >
                  ◀
                </button>
                <button
                  onClick={() => onPreviewStep(1)}
                  disabled={preview.at >= preview.sans.length}
                  title="Forward within the variation (→)"
                >
                  ▶
                </button>
              </span>
              <span className="ply-pill">
                move {preview.at} / {preview.sans.length}
              </span>
              <button className="btn preview-exit" onClick={onExitPreview} title="Esc also exits">
                ← Back to game
              </button>
            </div>
          )}

          <div className="move-controls">
            <span className="btn-group">
              <button onClick={() => setPly(0)} disabled={!game || gv.ply === 0} title="Start (Home)">
                |◀
              </button>
              <button onClick={() => step(-1)} disabled={!game || gv.ply === 0}>
                ◀ Prev
              </button>
              <button onClick={() => step(1)} disabled={!game || gv.ply >= plyCount}>
                Next ▶
              </button>
              <button
                onClick={() => setPly(plyCount)}
                disabled={!game || gv.ply >= plyCount}
                title="End (End)"
              >
                ▶|
              </button>
            </span>
            <span className="ply-pill">
              ply {gv.ply} / {plyCount}
            </span>
            <button className="btn" onClick={() => dispatch({ type: "toggleFlip" })}>
              Flip
            </button>
            <button
              className={`btn live-toggle${live.on ? " live-on" : ""}`}
              onClick={() => liveDispatch({ type: "toggle", fen })}
              title="Infinite engine analysis of the shown position — runs until switched off"
            >
              {live.on ? "■ Live analysis" : "Live analysis"}
            </button>
            <span className="kbd-hint">← → step · ↑ ↓ jump 5 · f flip · e explain</span>
          </div>

          {live.on && (
            <div className="live-strip" role="status">
              <span className="live-dot" aria-hidden />
              {liveCur ? (
                <>
                  <span className="live-score">{formatScore(liveCur, fen)}</span>
                  {liveCur.depth !== undefined && (
                    <span className="live-depth">d{liveCur.depth}</span>
                  )}
                  <span
                    className="live-line"
                    title={
                      pvLineFull
                        ? `${pvLineFull}\n${summarizeInfo(liveCur, fen)}`
                        : summarizeInfo(liveCur, fen)
                    }
                  >
                    {pvLineShort || "…"}
                  </span>
                  <button
                    className="btn live-add"
                    onClick={addPvAsVariation}
                    disabled={!canAddPv}
                    title={
                      canAddPv
                        ? `Insert the first ${Math.min(pvSans.length, PV_INSERT_PLIES)} moves of this line as a variation at the current move`
                        : editing === null
                          ? "Open a database game to add engine lines as variations"
                          : preview !== null
                            ? "Exit the variation preview first"
                            : gv.ply >= plyCount
                              ? "End of the mainline — nothing to vary here"
                              : "Waiting for an engine line"
                    }
                  >
                    + Add as variation
                  </button>
                </>
              ) : (
                <span className="live-text">engine thinking…</span>
              )}
            </div>
          )}

          {pendingVar && (
            <div className="var-offer">
              <span>
                Add {pendingVar.label} as a variation of {game?.sans[pendingVar.ply - 1]}?
              </span>
              <button className="btn" onClick={onAcceptVar}>
                Add as variation
              </button>
              <button className="btn" onClick={onDismissVar}>
                Dismiss
              </button>
            </div>
          )}
        </div>

        <div className="right-pane">
          {explainOn && preview && (
            <div className="preview-note">
              Variation preview — the explanation and eval bar track the main game and are
              paused. <button onClick={onExitPreview}>Back to game</button>
            </div>
          )}
          {explainOn && !preview && (
            <ExplainPanel
              explanation={explanation}
              explaining={explaining}
              voice={gv.voice}
              onVoice={(v) => dispatch({ type: "setVoice", voice: v })}
              hoverSentence={gv.hoverSentence}
              onHoverSentence={(i) => dispatch({ type: "hoverSentence", index: i })}
              selectedSquare={gv.selectedSquare}
              onExplain={onExplain}
              explainedPlies={explainedPlies}
              alertsExpanded={alertsExpanded}
              onToggleAlerts={() => setAlertsExpanded((v) => !v)}
            />
          )}
          <MovesPanel
            rows={rows}
            currentPly={gv.ply}
            evals={evalsMap}
            annotationMode={gv.annotationMode}
            onAnnotationMode={(m) => dispatch({ type: "setAnnotationMode", mode: m })}
            onSelectPly={setPly}
            editing={editing}
            repGlyphs={repGlyphs}
            onPreviewVariation={onPreviewVariation}
            previewVarIndex={preview?.varStartIndex ?? null}
            onNarrationHover={
              // COACH-row hover lights the current ply's evidence union
              // through the ONE hover pipeline (COACH_HOVER_INDEX);
              // meaningless while Explain is off or a preview repaints
              // the board (audit #4 kept: no stale overlays).
              explainOn && !preview
                ? (h) =>
                    dispatch({
                      type: "hoverSentence",
                      index: h ? COACH_HOVER_INDEX : null,
                    })
                : undefined
            }
          />
          {game && game.sans.length > 0 && (
            <div className="rep-footer">
              <span title="Adds the moves up to the current ply (whole game at ply 0) as training cards">
                {gv.ply > 0 ? `Line (first ${gv.ply} plies)` : "Mainline"} → repertoire:
              </span>
              <button onClick={() => onAddToRepertoire("white")}>as White</button>
              <button onClick={() => onAddToRepertoire("black")}>as Black</button>
            </div>
          )}
        </div>
      </div>

      {exportText !== null && (
        <div className="modal-overlay" onClick={() => setExportText(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Export PGN</h3>
            <textarea readOnly value={exportText} spellCheck={false} />
            <div className="modal-buttons">
              <button
                className="btn"
                onClick={() => {
                  void navigator.clipboard?.writeText(exportText);
                }}
              >
                Copy
              </button>
              <button className="btn" onClick={() => setExportText(null)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
