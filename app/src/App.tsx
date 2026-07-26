/**
 * App shell (design/handoff-1 §Screen 2): nav rail → main column
 * (header + view + status strip). The game view is the centrepiece;
 * every other capability keeps a rail home. The old tab bar is gone.
 */
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { parseFen } from "chessops/fen";
import { makeSan } from "chessops/san";
import { parseSquare } from "chessops/util";
import Board, { type BoardMovable } from "./Board";
import DatabaseView from "./DatabaseView";
import EndgameView from "./EndgameView";
import FirstRunOverlay, { markFirstRunSeen, shouldShowFirstRun } from "./FirstRunOverlay";
import GameView from "./GameView";
import Help from "./Help";
import ImportView from "./ImportView";
import JobsView from "./JobsView";
import { SyncsPlaceholder, TwicPlaceholder } from "./PlaceholderView";
import PrepView from "./PrepView";
import ProfileView from "./ProfileView";
import { usePromotionPicker } from "./PromotionPicker";
import SettingsView from "./SettingsView";
import TacticsView from "./TacticsView";
import TrainView, { type TrainBoardState } from "./TrainView";
import NavRail from "./shell/NavRail";
import StatusStrip from "./shell/StatusStrip";
import type { AnalysisRow } from "./lib/analyses";
import { getSavedAnnotationDisplay, saveAnnotationDisplay } from "./lib/annotationDisplay";
import {
  explainPosition,
  gameAnalyses,
  getGame,
  getGameTokens,
  getNarrationVoice,
  getSavedDbPath,
  getSavedVoice,
  jobsStatus,
  openDatabase,
  runJobs,
  saveVoice,
  setNarrationVoice,
  trainAddLine,
  trainSummary,
  updateGameTokens,
  type DbSummary,
  type GameDetail,
  type JobsStatus,
  type PlayerProfile,
  type TrainSummary,
} from "./lib/db";
import { getSavedNodes } from "./lib/engine";
import { onEngineDone, onEngineInfo } from "./lib/engine";
import {
  clampPly,
  gameFromSans,
  lastMoveAt,
  loadGame,
  type LoadedGame,
} from "./lib/game";
import {
  isEditableTarget,
  keyboardAction,
  railCollapsed,
  reduceGameView,
  type EditableTargetLike,
  type ExplanationJson,
  type GameViewState,
} from "./lib/gameView";
import type { PromoRole } from "./lib/promotion";
import type { ViewId } from "./lib/shell";
import { tacticsState as fetchTacticsState, type TacticsState } from "./lib/tactics";
import { insertVariation, type JsonToken } from "./lib/tokens";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const THEME_KEY = "silman.theme";
const TREATMENT_KEY = "silman.boardTreatment";
const EXPLAIN_KEY = "silman.explainOn";

function initialGameView(): GameViewState {
  return {
    ply: 0,
    hoverSentence: null,
    selectedSquare: null,
    voice: getSavedVoice(),
    annotationMode: getSavedAnnotationDisplay(),
    boardTreatment: localStorage.getItem(TREATMENT_KEY) === "instrument" ? "instrument" : "walnut",
    theme: localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark",
    flipped: false,
  };
}

/** Annotation-edit state for the currently loaded database game. */
interface AnnotState {
  gameId: number;
  startFen: string;
  tokens: JsonToken[];
  saved: JsonToken[];
}

interface PendingVariation {
  ply: number;
  san: string;
  label: string;
}

const VIEW_TITLES: Record<ViewId, string> = {
  database: "Database",
  game: "Game",
  tree: "Opening tree",
  search: "Position search",
  profile: "Player profile",
  prep: "Opponent prep",
  train: "Openings SRS",
  tactics: "Tactics",
  endgames: "Endgames",
  import: "Import PGN / SCID",
  twic: "TWIC ingest",
  syncs: "Account syncs",
  jobs: "Jobs",
  settings: "Settings",
};

export default function App() {
  const [view, setView] = useState<ViewId>("game");
  const [gv, dispatch] = useReducer(reduceGameView, undefined, initialGameView);
  const [showHelp, setShowHelp] = useState(false);
  const [showTour, setShowTour] = useState(shouldShowFirstRun);
  const [winWidth, setWinWidth] = useState(() => window.innerWidth);

  const [game, setGame] = useState<LoadedGame | null>(null);
  const [status, setStatus] = useState(
    "Open a game from the Database, or paste a PGN under Import.",
  );
  const [annot, setAnnot] = useState<AnnotState | null>(null);
  const [saving, setSaving] = useState(false);
  const [pendingVar, setPendingVar] = useState<PendingVariation | null>(null);
  const [analysisRows, setAnalysisRows] = useState<AnalysisRow[]>([]);
  const [profile, setProfile] = useState<PlayerProfile | null>(null);

  const [explainOn, setExplainOn] = useState(() => localStorage.getItem(EXPLAIN_KEY) !== "off");
  const [explanations, setExplanations] = useState<Map<number, ExplanationJson>>(new Map());
  const [explaining, setExplaining] = useState(false);

  const [dbSummary, setDbSummary] = useState<DbSummary | null>(null);
  const [trainSum, setTrainSum] = useState<TrainSummary | null>(null);
  const [tacticsSt, setTacticsSt] = useState<TacticsState | null>(null);
  const [jobs, setJobs] = useState<JobsStatus | null>(null);
  const [jobsError, setJobsError] = useState<string | null>(null);
  const [engineRunning, setEngineRunning] = useState(false);
  const [trainBoard, setTrainBoard] = useState<TrainBoardState | null>(null);

  const tokenReqRef = useRef(0);
  /** pending+running when the jobs worker went active (progress base). */
  const batchTotalRef = useRef<number | null>(null);

  const plyCount = game?.sans.length ?? 0;
  const fen = game ? game.fens[gv.ply] : START_FEN;
  const lastMove = game ? lastMoveAt(game, gv.ply) : undefined;

  /* ---- persisted view preferences ---- */
  useEffect(() => {
    document.documentElement.dataset.theme = gv.theme;
    localStorage.setItem(THEME_KEY, gv.theme);
  }, [gv.theme]);
  useEffect(() => {
    localStorage.setItem(TREATMENT_KEY, gv.boardTreatment);
  }, [gv.boardTreatment]);
  useEffect(() => {
    saveAnnotationDisplay(gv.annotationMode);
  }, [gv.annotationMode]);
  useEffect(() => {
    saveVoice(gv.voice);
    setNarrationVoice(gv.voice).catch(() => {}); // no database open — local only
  }, [gv.voice]);
  useEffect(() => {
    localStorage.setItem(EXPLAIN_KEY, explainOn ? "on" : "off");
  }, [explainOn]);

  /* ---- window resize (rail collapse) ---- */
  useEffect(() => {
    const onResize = () => setWinWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  /* ---- shell data: db auto-open, badges, jobs polling ---- */
  const refreshCounts = useCallback(() => {
    trainSummary().then(setTrainSum).catch(() => setTrainSum(null));
    fetchTacticsState().then(setTacticsSt).catch(() => setTacticsSt(null));
  }, []);

  useEffect(() => {
    // Auto-open the saved database so the shell shows real data at launch.
    openDatabase(getSavedDbPath())
      .then((s) => {
        setDbSummary(s);
        refreshCounts();
        getNarrationVoice()
          .then((v) => dispatch({ type: "setVoice", voice: v }))
          .catch(() => {});
      })
      .catch(() => {}); // no database yet — badges stay empty
  }, [refreshCounts]);

  useEffect(() => {
    // Counts go stale while training; refresh on every view switch.
    refreshCounts();
  }, [view, refreshCounts]);

  useEffect(() => {
    const tick = () => {
      jobsStatus()
        .then((j) => {
          setJobs(j);
          if (j.workerActive && batchTotalRef.current === null) {
            batchTotalRef.current = j.pending + j.running;
          } else if (!j.workerActive) {
            batchTotalRef.current = null;
          }
        })
        .catch(() => setJobs(null));
    };
    tick();
    const t = setInterval(tick, 3000);
    return () => clearInterval(t);
  }, [dbSummary]);

  // Engine activity (the status-strip dot) from the UCI manager's events.
  useEffect(() => {
    const unsubs: Array<() => void> = [];
    onEngineInfo(() => setEngineRunning(true))
      .then((u) => unsubs.push(u))
      .catch(() => {}); // not running inside Tauri (vite-only dev)
    onEngineDone(() => setEngineRunning(false))
      .then((u) => unsubs.push(u))
      .catch(() => {});
    return () => unsubs.forEach((u) => u());
  }, []);

  /* ---- game loading ---- */
  const applyGame = useCallback((g: LoadedGame, label: string, warning?: string, atPly = 0) => {
    setGame(g);
    dispatch({ type: "gameLoaded", ply: clampPly(atPly, g), plyCount: g.sans.length });
    setStatus(label + (warning ? ` ${warning}` : ""));
    setExplanations(new Map());
    setRevealedQuiet(new Set());
    setPendingVar(null);
  }, []);

  const doLoad = useCallback(
    (text: string) => {
      const res = loadGame(text);
      if (!res.ok) {
        setStatus(res.error);
        return;
      }
      const w = res.game.headers["White"] ?? "?";
      const b = res.game.headers["Black"] ?? "?";
      tokenReqRef.current++;
      setAnnot(null);
      setAnalysisRows([]);
      applyGame(res.game, `${w} — ${b}, ${res.game.sans.length} plies.`, res.warning);
      setView("game");
    },
    [applyGame],
  );

  const loadDbGame = useCallback(
    (detail: GameDetail, atPly = 0) => {
      const headers: Record<string, string> = {
        White: detail.white,
        Black: detail.black,
        Event: detail.event,
        Site: detail.site,
        Date: detail.date ?? "?",
        Round: detail.round ?? "?",
        Result: detail.result,
      };
      if (detail.eco) headers["ECO"] = detail.eco;
      const res = gameFromSans(detail.sans, detail.startFen, headers);
      if (!res.ok) {
        setStatus(`Failed to load game #${detail.id}: ${res.error}`);
        return;
      }
      const elos =
        detail.whiteElo || detail.blackElo
          ? ` (${detail.whiteElo ?? "?"}–${detail.blackElo ?? "?"})`
          : "";
      setAnnot(null);
      setAnalysisRows([]);
      applyGame(
        res.game,
        `#${detail.id} ${detail.white} — ${detail.black}${elos}, ${detail.result}, ${res.game.sans.length} plies.`,
        res.warning,
        atPly,
      );
      setView("game");
      const req = ++tokenReqRef.current;
      getGameTokens(detail.id)
        .then((gt) => {
          if (tokenReqRef.current !== req) return;
          setAnnot({ gameId: detail.id, startFen: gt.startFen, tokens: gt.tokens, saved: gt.tokens });
        })
        .catch((e) => setStatus((s) => `${s} (annotations unavailable: ${e})`));
      gameAnalyses(detail.id)
        .then((rows) => {
          if (tokenReqRef.current !== req) return;
          setAnalysisRows(rows);
        })
        .catch(() => {}); // eval display is best-effort
    },
    [applyGame],
  );

  const loadDbGameAt = useCallback(
    async (gameId: number, atPly: number) => {
      try {
        loadDbGame(await getGame(gameId), atPly);
      } catch (e) {
        setStatus(String(e));
      }
    },
    [loadDbGame],
  );

  // Deep link: #game=123&ply=24&theme=light&treatment=instrument&voice=neutral
  // applies once after the database opens. Handy for dev, demos and docs.
  const hashApplied = useRef(false);
  useEffect(() => {
    if (!dbSummary || hashApplied.current) return;
    const h = new URLSearchParams(window.location.hash.slice(1));
    if ([...h.keys()].length === 0) return;
    hashApplied.current = true;
    const theme = h.get("theme");
    if (theme === "light" || theme === "dark") dispatch({ type: "setTheme", theme });
    const treatment = h.get("treatment");
    if (treatment === "walnut" || treatment === "instrument") {
      dispatch({ type: "setTreatment", treatment });
    }
    const voice = h.get("voice");
    if (voice === "coach" || voice === "neutral") dispatch({ type: "setVoice", voice });
    const gameId = Number(h.get("game"));
    if (Number.isFinite(gameId) && gameId > 0) {
      void loadDbGameAt(gameId, Number(h.get("ply")) || 0);
    }
  }, [dbSummary, loadDbGameAt]);

  const reloadCurrent = useCallback(() => {
    if (annot) void loadDbGameAt(annot.gameId, gv.ply);
  }, [annot, gv.ply, loadDbGameAt]);

  /* ---- stepping ---- */
  const step = useCallback(
    (delta: number) => {
      if (game) dispatch({ type: "step", delta, plyCount: game.sans.length });
    },
    [game],
  );
  const setPlyTo = useCallback(
    (ply: number) => {
      if (game) dispatch({ type: "setPly", ply, plyCount: game.sans.length });
    },
    [game],
  );

  /* ---- explain (cache per ply; both voices arrive at once) ---- */
  // Quiet positions keep the empty state until the user explicitly asks;
  // fired screens show their explanation as soon as the (static, free)
  // analysis lands.
  const [revealedQuiet, setRevealedQuiet] = useState<Set<number>>(new Set());
  const fetched = explainOn ? (explanations.get(gv.ply) ?? null) : null;
  const currentExplanation =
    fetched && (fetched.tag !== "QUIET POSITION" || revealedQuiet.has(gv.ply)) ? fetched : null;
  const explainedPlies = useMemo(
    () => [...explanations.keys()].sort((a, b) => a - b),
    [explanations],
  );

  const doExplain = useCallback(async () => {
    // An explicit request also reveals a quiet position's explanation.
    setRevealedQuiet((r) => (r.has(gv.ply) ? r : new Set(r).add(gv.ply)));
    if (explaining || explanations.has(gv.ply)) return;
    const ply = gv.ply;
    setExplaining(true);
    try {
      const res = await explainPosition(fen, gv.voice);
      setExplanations((m) => new Map(m).set(ply, res.explanation));
    } catch (e) {
      setStatus(`Explain failed: ${e}`);
    } finally {
      setExplaining(false);
    }
  }, [explaining, explanations, gv.ply, fen, gv.voice]);

  // The tactical screen itself is static and free: fetch the current
  // ply's explanation automatically so fired screens narrate without a
  // keypress. The ENGINE still never runs from here (CLAUDE.md #6).
  useEffect(() => {
    if (!explainOn || !game || explanations.has(gv.ply)) return;
    let stale = false;
    const ply = gv.ply;
    explainPosition(fen, gv.voice)
      .then((res) => {
        if (!stale) setExplanations((m) => new Map(m).set(ply, res.explanation));
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  }, [explainOn, game, explanations, gv.ply, fen, gv.voice]);

  const toggleExplain = useCallback(() => setExplainOn((v) => !v), []);

  /* ---- board move input (annotatable db games → variations) ---- */
  const boardMoveRef = useRef<(orig: string, dest: string, promoRole?: PromoRole) => void>(
    () => {},
  );
  const promo = usePromotionPicker((orig, dest, role) => boardMoveRef.current(orig, dest, role));

  const handleBoardMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!game || !annot) return;
      if (!promoRole && promo.guard(game.fens[gv.ply], orig, dest)) return;
      const setup = parseFen(game.fens[gv.ply]);
      if (setup.isErr) return;
      const p = Chess.fromSetup(setup.unwrap());
      if (p.isErr) return;
      const pos = p.unwrap();
      const from = parseSquare(orig);
      const to = parseSquare(dest);
      if (from === undefined || to === undefined) return;
      const promotion = pos.board.get(from)?.role === "pawn" && promoRole ? promoRole : undefined;
      const move = normalizeMove(pos, { from, to, promotion });
      if (!pos.isLegal(move)) return;
      const san = makeSan(pos, move);
      if (gv.ply < game.sans.length && san === game.sans[gv.ply]) {
        step(1);
        return;
      }
      if (gv.ply >= game.sans.length) {
        setStatus("End of the mainline — a board move can only vary an existing move.");
        return;
      }
      const num = pos.turn === "white" ? `${pos.fullmoves}.` : `${pos.fullmoves}...`;
      setPendingVar({ ply: gv.ply + 1, san, label: `${num} ${san}` });
    },
    [game, annot, gv.ply, step, promo],
  );
  boardMoveRef.current = handleBoardMove;

  const movable = useMemo((): BoardMovable | undefined => {
    if (!game || !annot) return undefined;
    const setup = parseFen(fen);
    if (setup.isErr) return undefined;
    const p = Chess.fromSetup(setup.unwrap());
    if (p.isErr) return undefined;
    const pos = p.unwrap();
    return { color: pos.turn, dests: chessgroundDests(pos), onMove: handleBoardMove };
  }, [game, annot, fen, handleBoardMove]);

  const acceptPendingVar = useCallback(() => {
    if (!pendingVar) return;
    setAnnot((a) =>
      a ? { ...a, tokens: insertVariation(a.tokens, pendingVar.ply, [pendingVar.san]) } : a,
    );
    setPendingVar(null);
  }, [pendingVar]);

  const saveAnnotations = useCallback(async () => {
    if (!annot) return;
    setSaving(true);
    try {
      await updateGameTokens(annot.gameId, annot.tokens);
      loadDbGame(await getGame(annot.gameId), gv.ply);
      setStatus(`Annotations saved for game #${annot.gameId}.`);
    } catch (e) {
      setStatus(`Save failed: ${e}`);
    } finally {
      setSaving(false);
    }
  }, [annot, gv.ply, loadDbGame]);

  const revertAnnotations = useCallback(() => {
    setAnnot((a) => (a ? { ...a, tokens: a.saved } : a));
    setPendingVar(null);
  }, []);

  /* ---- repertoire ---- */
  const addLineToRepertoire = useCallback(
    async (color: "white" | "black") => {
      if (!game) return;
      const sans = gv.ply > 0 ? game.sans.slice(0, gv.ply) : game.sans;
      if (sans.length === 0) return;
      try {
        const res = await trainAddLine(color, sans, game.fens[0]);
        setStatus(
          `Repertoire "${res.repertoire}": ${res.cardsAdded} new cards, ` +
            `${res.cardsExisting} positions already covered (${sans.length} plies).`,
        );
        refreshCounts();
      } catch (e) {
        setStatus(`Add to repertoire failed: ${e}`);
      }
    },
    [game, gv.ply, refreshCounts],
  );

  /* ---- keyboard map (game view only; never in inputs/modals) ---- */
  useEffect(() => {
    if (view !== "game") return;
    const onKey = (e: KeyboardEvent) => {
      if (showHelp || showTour || promo.active) return;
      const act = keyboardAction(e.key, {
        editable: isEditableTarget(e.target as EditableTargetLike | null),
        modifier: e.metaKey || e.ctrlKey || e.altKey,
      });
      if (!act) return;
      e.preventDefault();
      switch (act) {
        case "next":
          step(1);
          break;
        case "prev":
          step(-1);
          break;
        case "fwd5":
          step(5);
          break;
        case "back5":
          step(-5);
          break;
        case "start":
          setPlyTo(0);
          break;
        case "end":
          setPlyTo(plyCount);
          break;
        case "flip":
          dispatch({ type: "toggleFlip" });
          break;
        case "explain":
          if (!explainOn) setExplainOn(true);
          void doExplain();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, showHelp, showTour, promo.active, step, setPlyTo, plyCount, explainOn, doExplain]);

  /* ---- jobs (view + strip) ---- */
  const doRunJobs = useCallback(async () => {
    setJobsError(null);
    try {
      await runJobs();
      const j = await jobsStatus();
      setJobs(j);
      batchTotalRef.current = j.pending + j.running;
    } catch (e) {
      setJobsError(String(e));
    }
  }, []);

  const batchProgress = useMemo(() => {
    if (!jobs?.workerActive || batchTotalRef.current === null || batchTotalRef.current === 0) {
      return null;
    }
    const remaining = jobs.pending + jobs.running;
    return {
      label: "ENGINE JOBS",
      fraction: Math.max(0, Math.min(1, 1 - remaining / batchTotalRef.current)),
    };
  }, [jobs]);

  /* ---- train board (main-area board while a review session runs) ---- */
  const trainPromo = usePromotionPicker((orig, dest, role) => {
    trainBoard?.movable?.onMove(orig, dest, role);
  });
  const trainMovable = useMemo((): BoardMovable | undefined => {
    if (!trainBoard?.movable) return undefined;
    const m = trainBoard.movable;
    return {
      ...m,
      onMove: (orig, dest) => {
        if (!trainPromo.guard(trainBoard.fen, orig, dest)) m.onMove(orig, dest);
      },
    };
  }, [trainBoard, trainPromo]);

  /* ---- shell chrome data ---- */
  const collapsed = railCollapsed(winWidth);
  const dbLine = dbSummary
    ? `${dbSummary.path.split(/[\\/]/).pop() ?? dbSummary.path} · ${dbSummary.games.toLocaleString()} games`
    : null;
  const railData = {
    dbGames: dbSummary?.games ?? null,
    explainOn,
    profile,
    train: trainSum,
    tactics: tacticsSt,
    jobs,
  };
  const engineDetail = `${jobs?.engine ?? "Stockfish"} · nodes ${getSavedNodes().toLocaleString()}`;

  const pageView = (() => {
    switch (view) {
      case "database":
      case "tree":
      case "search":
        return (
          <DatabaseView
            currentFen={fen}
            game={game}
            ply={gv.ply}
            onLoadGame={loadDbGame}
            onAdvance={() => step(1)}
            summary={dbSummary}
            onSummary={setDbSummary}
            focus={view === "database" ? "all" : view}
          />
        );
      case "profile":
        return (
          <ProfileView
            profile={profile}
            onProfileBuilt={setProfile}
            onLoadGameAt={(id, ply) => void loadDbGameAt(id, ply)}
          />
        );
      case "prep":
        return <PrepView onLoadGameAt={(id, ply) => void loadDbGameAt(id, ply)} profile={profile} />;
      case "train":
        return (
          <div className="trainer-layout">
            {trainBoard && (
              <div className="trainer-board">
                <Board
                  fen={trainBoard.fen}
                  orientation={trainBoard.orientation}
                  movable={trainMovable}
                  shapes={trainBoard.shapes}
                  treatment={gv.boardTreatment}
                  size={488}
                />
                {trainPromo.element}
              </div>
            )}
            <TrainView onSummary={setTrainSum} onBoard={setTrainBoard} />
          </div>
        );
      case "tactics":
        return <TacticsView profile={profile} />;
      case "endgames":
        return <EndgameView />;
      case "import":
        return <ImportView onLoad={doLoad} status={status} />;
      case "twic":
        return <TwicPlaceholder />;
      case "syncs":
        return <SyncsPlaceholder />;
      case "jobs":
        return (
          <JobsView
            jobs={jobs}
            running={jobs?.workerActive ?? false}
            onRunJobs={() => void doRunJobs()}
            error={jobsError}
          />
        );
      case "settings":
        return (
          <SettingsView
            voice={gv.voice}
            onVoice={(v) => dispatch({ type: "setVoice", voice: v })}
            annotationMode={gv.annotationMode}
            onAnnotationMode={(m) => dispatch({ type: "setAnnotationMode", mode: m })}
            treatment={gv.boardTreatment}
            onTreatment={(t) => dispatch({ type: "setTreatment", treatment: t })}
            theme={gv.theme}
            onTheme={(t) => dispatch({ type: "setTheme", theme: t })}
          />
        );
      case "game":
        return null;
    }
  })();

  return (
    <div className="shell">
      <NavRail
        active={view}
        collapsed={collapsed}
        dbLine={dbLine}
        data={railData}
        onNavigate={setView}
        onToggleExplain={toggleExplain}
        onHelp={() => setShowHelp(true)}
      />
      <div className="shell-main">
        {view === "game" ? (
          <GameView
            game={game}
            fen={fen}
            lastMove={lastMove}
            movable={movable}
            promoElement={promo.element}
            gv={gv}
            dispatch={dispatch}
            plyCount={plyCount}
            gameId={annot?.gameId ?? null}
            editing={
              annot
                ? {
                    tokens: annot.tokens,
                    onChange: (tokens) => setAnnot((a) => (a ? { ...a, tokens } : a)),
                    dirty: annot.tokens !== annot.saved,
                    saving,
                    onSave: () => void saveAnnotations(),
                    onRevert: revertAnnotations,
                  }
                : null
            }
            analysisRows={analysisRows}
            explainOn={explainOn}
            explanation={currentExplanation}
            explaining={explaining}
            explainedPlies={explainedPlies}
            onExplain={() => void doExplain()}
            pendingVar={pendingVar}
            onAcceptVar={acceptPendingVar}
            onDismissVar={() => setPendingVar(null)}
            onAddToRepertoire={(c) => void addLineToRepertoire(c)}
            onReload={reloadCurrent}
            onStatus={setStatus}
          />
        ) : (
          <>
            <header className="header-bar simple">
              <span className="header-title">{VIEW_TITLES[view]}</span>
            </header>
            <div className="page-scroll">{pageView}</div>
          </>
        )}
        <StatusStrip
          engineRunning={engineRunning}
          engineDetail={engineDetail}
          jobs={jobs}
          batchProgress={batchProgress}
          train={trainSum}
          message={status}
          onNudge={() => setView("train")}
        />
      </div>

      {showHelp && <Help onClose={() => setShowHelp(false)} />}
      {showTour && !showHelp && (
        <FirstRunOverlay
          onClose={() => {
            markFirstRunSeen();
            setShowTour(false);
          }}
          onOpenHelp={() => {
            markFirstRunSeen();
            setShowTour(false);
            setShowHelp(true);
          }}
        />
      )}
    </div>
  );
}
