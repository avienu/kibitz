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
import { type BoardMovable } from "./Board";
import DatabaseScreen from "./DatabaseScreen";
import EndgameView from "./EndgameView";
import HomeView from "./HomeView";
import OpeningTreeView from "./OpeningTreeView";
import PositionSearchView from "./PositionSearchView";
import FirstRunOverlay, { markFirstRunSeen, shouldShowFirstRun } from "./FirstRunOverlay";
import GameView from "./GameView";
import Help from "./Help";
import ImportView from "./ImportView";
import JobsView from "./JobsView";
import PrepView from "./PrepView";
import SyncsView from "./SyncsView";
import TwicView from "./TwicView";
import ProfileView from "./ProfileView";
import { usePromotionPicker } from "./PromotionPicker";
import SettingsView from "./SettingsView";
import TacticsView from "./TacticsView";
import TrainView from "./TrainView";
import NavRail from "./shell/NavRail";
import StatusStrip from "./shell/StatusStrip";
import type { AnalysisRow } from "./lib/analyses";
import { getSavedAnnotationDisplay, saveAnnotationDisplay } from "./lib/annotationDisplay";
import {
  explainPosition,
  fetchDbSummary,
  gameAnalyses,
  getGame,
  getGameTokens,
  getNarrationVoice,
  getSavedDbPath,
  getSavedVoice,
  jobsStatus,
  openDatabase,
  repertoireMarks,
  runJobs,
  saveVoice,
  setNarrationVoice,
  touchLastGame,
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
  netProgress as fetchNetProgress,
  netStripProgress,
  railNetBadges,
  twicAutoSyncCheck,
  type NetBadges,
  type NetProgress,
} from "./lib/net";
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
  type GameViewAction,
  type GameViewState,
} from "./lib/gameView";
import type { MovesRow } from "./lib/movesView";
import {
  enterPreview,
  previewFen,
  previewLastMove,
  stepPreview,
  type VariationPreview,
} from "./lib/preview";
import type { PromoRole } from "./lib/promotion";
import type { RepertoireMark } from "./lib/repMarks";
import { viewKeyHints, type ViewId, type ViewParams } from "./lib/shell";
import { tacticsState as fetchTacticsState, type TacticsState } from "./lib/tactics";
import { insertVariation, type JsonToken } from "./lib/tokens";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const THEME_KEY = "kibitz.theme";
const TREATMENT_KEY = "kibitz.boardTreatment";
const EXPLAIN_KEY = "kibitz.explainOn";

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
  /** Generated coach narrations by ply (display-only). */
  narrations: Map<number, string>;
}

interface PendingVariation {
  ply: number;
  san: string;
  label: string;
}

const VIEW_TITLES: Record<ViewId, string> = {
  home: "Home",
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
  // Home is the startup screen (round-2 maintainer ruling: Direction A).
  const [view, setView] = useState<ViewId>("home");
  // One-shot per-screen navigation params (lib/shell.ts ViewParams):
  // e.g. navigate("profile", { claim }) pre-selects a claim's evidence.
  const [viewParams, setViewParams] = useState<ViewParams>({});
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
  const [repMarks, setRepMarks] = useState<RepertoireMark[]>([]);
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
  const [netProg, setNetProg] = useState<NetProgress | null>(null);
  const [netBadges, setNetBadges] = useState<NetBadges | null>(null);

  const tokenReqRef = useRef(0);
  /** pending+running when the jobs worker went active (progress base). */
  const batchTotalRef = useRef<number | null>(null);

  /** Variation preview (run-9 round 2): while non-null, the BOARD shows
   * the previewed line; gv.ply and everything keyed to it (explain,
   * eval, annotations) stay on the main game. */
  const [preview, setPreview] = useState<VariationPreview | null>(null);

  const plyCount = game?.sans.length ?? 0;
  // `fen` is the MAIN-game position — explain/annotation/eval stay keyed
  // to it; `shownFen` is what the board (and live analysis) displays.
  const fen = game ? game.fens[gv.ply] : START_FEN;
  const lastMove = game ? lastMoveAt(game, gv.ply) : undefined;
  const shownFen = preview ? previewFen(preview) : fen;
  const shownLastMove = preview ? previewLastMove(preview) : lastMove;

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

  /** Route to a screen, optionally with one-shot params (ViewParams). */
  const navigate = useCallback((id: ViewId, params?: ViewParams) => {
    setView(id);
    setViewParams(params ?? {});
  }, []);

  /* ---- shell data: db auto-open, badges, jobs polling ---- */
  const refreshCounts = useCallback(() => {
    trainSummary().then(setTrainSum).catch(() => setTrainSum(null));
    fetchTacticsState().then(setTacticsSt).catch(() => setTacticsSt(null));
  }, []);

  useEffect(() => {
    // Auto-open the saved database so the shell shows real data at launch.
    // A #db=<path> deep link opens that database instead (not persisted).
    const dbOverride = new URLSearchParams(window.location.hash.slice(1)).get("db");
    openDatabase(dbOverride || getSavedDbPath())
      .then((s) => {
        setDbSummary(s);
        refreshCounts();
        getNarrationVoice()
          .then((v) => dispatch({ type: "setVoice", voice: v }))
          .catch(() => {});
        // TWIC auto-download hook: quietly syncs NEW issues only when the
        // user enabled the toggle (netops.rs; no-op otherwise).
        twicAutoSyncCheck().catch(() => {});
      })
      .catch(() => {}); // no database yet — badges stay empty
  }, [refreshCounts]);

  useEffect(() => {
    // Counts go stale while training; refresh on every view switch.
    refreshCounts();
  }, [view, refreshCounts]);

  // When the jobs worker finishes, results (fresh evals, fold-back
  // narrations) must appear in the open game without a manual reload —
  // otherwise Re-analyze looks like it "did nothing" (run-8 user report).
  const workerWasActive = useRef(false);
  useEffect(() => {
    const tick = () => {
      jobsStatus()
        .then((j) => {
          if (workerWasActive.current && !j.workerActive) {
            reloadCurrentRef.current();
            refreshCounts();
          }
          workerWasActive.current = j.workerActive;
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
    // Boolean dep: dbSummary's identity changes on every counts refresh
    // during a sync; this poller only cares whether a database is open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dbSummary !== null]);

  // Network worker (TWIC downloads / account syncs): poll its in-memory
  // progress (cheap, no db) and refresh the rail badges when a job ends.
  // While a sync is importing, db_summary is re-polled on this SAME
  // cadence — the one shared refresh path for every game-count display
  // (rail badge, Database header, list totals), so counts stop drifting
  // apart mid-sync (audit #8).
  const netWasActive = useRef(false);
  const dbOpenRef = useRef(false);
  dbOpenRef.current = dbSummary !== null;
  useEffect(() => {
    if (!dbSummary) return;
    const refreshBadges = () =>
      railNetBadges().then(setNetBadges).catch(() => setNetBadges(null));
    const refreshDbCounts = () =>
      fetchDbSummary()
        .then((s) => {
          if (dbOpenRef.current) setDbSummary(s);
        })
        .catch(() => {}); // counts refresh is best-effort
    refreshBadges();
    const tick = () => {
      fetchNetProgress()
        .then((p) => {
          const active = p?.active ?? false;
          if (netWasActive.current && !active) refreshBadges();
          // During a sync AND once when it finishes: one final refresh
          // lands the settled counts everywhere at the same moment.
          if (active || (netWasActive.current && !active)) refreshDbCounts();
          netWasActive.current = active;
          setNetProg(p);
        })
        .catch(() => setNetProg(null));
    };
    tick();
    const t = setInterval(tick, 3000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dbSummary !== null]);

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
    setPreview(null);
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
      setRepMarks([]); // pasted PGN has no db identity — no marks
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
      if (detail.openingName) headers["Opening"] = detail.openingName;
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
          setAnnot({
            gameId: detail.id,
            startFen: gt.startFen,
            tokens: gt.tokens,
            saved: gt.tokens,
            narrations: new Map(gt.narrations.map((n) => [n.ply, n.text])),
          });
        })
        .catch((e) => setStatus((s) => `${s} (annotations unavailable: ${e})`));
      gameAnalyses(detail.id)
        .then((rows) => {
          if (tokenReqRef.current !== req) return;
          setAnalysisRows(rows);
        })
        .catch(() => {}); // eval display is best-effort
      setRepMarks([]);
      repertoireMarks(detail.id)
        .then((marks) => {
          if (tokenReqRef.current !== req) return;
          setRepMarks(marks);
        })
        .catch(() => {}); // marks are best-effort decoration
    },
    [applyGame],
  );

  const loadDbGameAt = useCallback(
    async (gameId: number, atPly: number, flipped?: boolean) => {
      try {
        loadDbGame(await getGame(gameId), atPly);
        // Resume restores the board orientation you left the game in.
        if (flipped !== undefined) dispatch({ type: "setFlipped", flipped });
      } catch (e) {
        setStatus(String(e));
      }
    },
    [loadDbGame],
  );

  // Deep link: #game=123&ply=24&theme=light&treatment=instrument&voice=neutral
  // &screen=database — applies once after the database opens. Handy for
  // dev, demos and automated screenshots of every screen.
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
    // screen=home|database|tree|search|profile|prep|tactics|srs|endgames|
    // settings|help ("srs" is the rail's Openings SRS = view id "train";
    // "help" opens the overlay). A game=… link below still wins.
    const screen = h.get("screen");
    if (screen) {
      const views: Record<string, ViewId> = {
        home: "home",
        database: "database",
        tree: "tree",
        search: "search",
        profile: "profile",
        prep: "prep",
        tactics: "tactics",
        srs: "train",
        endgames: "endgames",
        settings: "settings",
      };
      if (screen === "help") setShowHelp(true);
      else if (views[screen]) {
        const params: ViewParams = {};
        const player = h.get("player");
        if (player) params.player = player;
        const opponent = h.get("opponent");
        if (opponent) params.opponent = opponent;
        const claim = h.get("claim");
        if (claim) params.claim = claim;
        navigate(views[screen], params);
      }
    }
    const gameId = Number(h.get("game"));
    if (Number.isFinite(gameId) && gameId > 0) {
      void loadDbGameAt(gameId, Number(h.get("ply")) || 0);
    }
  }, [dbSummary, loadDbGameAt, navigate]);

  // Feed Home's Continue card: record the game/ply on the board while the
  // user is actually in the game view (debounced across rapid stepping).
  const annotGameId = annot?.gameId ?? null;
  useEffect(() => {
    if (view !== "game" || annotGameId === null) return;
    const t = setTimeout(() => {
      touchLastGame(annotGameId, gv.ply, gv.flipped).catch(() => {}); // best-effort meta write
    }, 400);
    return () => clearTimeout(t);
  }, [view, annotGameId, gv.ply, gv.flipped]);

  const reloadCurrent = useCallback(() => {
    if (annot) void loadDbGameAt(annot.gameId, gv.ply);
  }, [annot, gv.ply, loadDbGameAt]);
  // Stable handle for the jobs poller (defined before reloadCurrent).
  const reloadCurrentRef = useRef(reloadCurrent);
  reloadCurrentRef.current = reloadCurrent;

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

  /* ---- variation preview (exit rules: any MAIN-game navigation, the
   * pill, Esc, or loading a game exits; ←/→ step within it) ---- */
  const previewVariation = useCallback(
    (row: Extract<MovesRow, { kind: "variation" }>) => {
      if (!game) return;
      const branchFen = game.fens[row.branchPly - 1];
      if (branchFen === undefined) return;
      const p = enterPreview(branchFen, row);
      if (p) setPreview(p);
      else setStatus("This variation has no playable moves to preview.");
    },
    [game],
  );
  const previewStepBy = useCallback(
    (delta: number) => setPreview((p) => (p ? stepPreview(p, delta) : p)),
    [],
  );
  const exitPreview = useCallback(() => setPreview(null), []);
  /** GameView's dispatch: main-game navigation also exits the preview. */
  const gvDispatch = useCallback((a: GameViewAction) => {
    if (a.type === "step" || a.type === "setPly" || a.type === "gameLoaded") setPreview(null);
    dispatch(a);
  }, []);

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
      const res = await explainPosition(
        fen,
        gv.voice,
        game && gv.ply > 0 ? game.fens[gv.ply - 1] : undefined,
        game && gv.ply > 0 ? game.sans[gv.ply - 1] : undefined,
      );
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
    explainPosition(
      fen,
      gv.voice,
      game && gv.ply > 0 ? game.fens[gv.ply - 1] : undefined,
      game && gv.ply > 0 ? game.sans[gv.ply - 1] : undefined,
    )
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
      if (preview && e.key === "Escape") {
        e.preventDefault();
        setPreview(null);
        return;
      }
      const act = keyboardAction(e.key, {
        editable: isEditableTarget(e.target as EditableTargetLike | null),
        modifier: e.metaKey || e.ctrlKey || e.altKey,
      });
      if (!act) return;
      e.preventDefault();
      if (preview) {
        // ←/→ step WITHIN the preview; explain stays paused; any other
        // main-game navigation exits the preview and then performs.
        if (act === "next" || act === "prev") {
          previewStepBy(act === "next" ? 1 : -1);
          return;
        }
        if (act === "explain") return;
        if (act !== "flip") setPreview(null);
      }
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
  }, [
    view,
    showHelp,
    showTour,
    promo.active,
    step,
    setPlyTo,
    plyCount,
    explainOn,
    doExplain,
    preview,
    previewStepBy,
  ]);

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
    const total = batchTotalRef.current;
    return {
      label: "ENGINE JOBS",
      fraction: Math.max(0, Math.min(1, 1 - remaining / total)),
      total,
      remaining,
    };
  }, [jobs]);

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
    twicLatestImported: netBadges?.twicLatestImported ?? null,
    syncAccounts: netBadges ? netBadges.accountsConfigured : null,
  };
  const engineDetail = `${jobs?.engine ?? "Stockfish"} · nodes ${getSavedNodes().toLocaleString()}`;

  // Screens that render their own header bar (round-2 layout owners).
  const selfHeaded =
    view === "database" ||
    view === "tree" ||
    view === "search" ||
    view === "train" ||
    view === "endgames" ||
    view === "settings" ||
    view === "profile" ||
    view === "prep" ||
    view === "tactics" ||
    view === "twic";

  const pageView = (() => {
    switch (view) {
      case "home":
        return (
          <HomeView
            dbOpen={dbSummary !== null}
            batchFraction={batchProgress?.fraction ?? null}
            netProgress={netProg}
            onNavigate={navigate}
            onOpenGame={(id, ply, flipped) => void loadDbGameAt(id, ply, flipped)}
          />
        );
      case "database":
        return (
          <DatabaseScreen
            summary={dbSummary}
            onSummary={setDbSummary}
            onLoadGame={loadDbGame}
            jobs={jobs}
            batch={batchProgress}
            onStatus={setStatus}
          />
        );
      case "tree":
        return (
          <OpeningTreeView
            dbOpen={dbSummary !== null}
            treatment={gv.boardTreatment}
            onOpenGameAt={(id, ply) => void loadDbGameAt(id, ply)}
          />
        );
      case "search":
        return (
          <PositionSearchView
            dbOpen={dbSummary !== null}
            treatment={gv.boardTreatment}
            onOpenGameAt={(id, ply) => void loadDbGameAt(id, ply)}
          />
        );
      case "profile":
        return (
          <ProfileView
            profile={profile}
            onProfileBuilt={setProfile}
            onLoadGameAt={(id, ply) => void loadDbGameAt(id, ply)}
            claim={viewParams.claim ?? null}
            opponent={viewParams.opponent ?? null}
            initialPlayer={viewParams.player ?? null}
            onNavigate={navigate}
          />
        );
      case "prep":
        return (
          <PrepView
            onLoadGameAt={(id, ply) => void loadDbGameAt(id, ply)}
            profile={profile}
            opponent={viewParams.opponent ?? null}
            onNavigate={navigate}
            treatment={gv.boardTreatment}
          />
        );
      case "train":
        return <TrainView onSummary={setTrainSum} treatment={gv.boardTreatment} />;
      case "tactics":
        return (
          <TacticsView
            profile={profile}
            seedClaim={viewParams.claim ?? null}
            voice={gv.voice}
            onVoice={(v) => dispatch({ type: "setVoice", voice: v })}
            treatment={gv.boardTreatment}
          />
        );
      case "endgames":
        return <EndgameView treatment={gv.boardTreatment} />;
      case "import":
        return <ImportView onLoad={doLoad} status={status} />;
      case "twic":
        return <TwicView progress={netProg} />;
      case "syncs":
        return <SyncsView progress={netProg} />;
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
        onNavigate={navigate}
        onToggleExplain={toggleExplain}
        onHelp={() => setShowHelp(true)}
      />
      <div className="shell-main">
        {view === "game" ? (
          <GameView
            game={game}
            fen={shownFen}
            lastMove={shownLastMove}
            movable={preview ? undefined : movable}
            promoElement={promo.element}
            gv={gv}
            dispatch={gvDispatch}
            plyCount={plyCount}
            gameId={annot?.gameId ?? null}
            editing={
              annot
                ? {
                    tokens: annot.tokens,
                    narrations: annot.narrations,
                    onChange: (tokens) => setAnnot((a) => (a ? { ...a, tokens } : a)),
                    dirty: annot.tokens !== annot.saved,
                    saving,
                    onSave: () => void saveAnnotations(),
                    onRevert: revertAnnotations,
                  }
                : null
            }
            analysisRows={analysisRows}
            repMarks={repMarks}
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
            preview={preview}
            onPreviewVariation={previewVariation}
            onPreviewStep={previewStepBy}
            onExitPreview={exitPreview}
          />
        ) : selfHeaded ? (
          pageView
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
          batchProgress={batchProgress ?? netStripProgress(netProg)}
          train={trainSum}
          message={status}
          keyHints={viewKeyHints(view)}
          onNudge={() => setView("train")}
        />
      </div>

      {showHelp && (
        <Help
          onClose={() => setShowHelp(false)}
          onReplayTour={() => {
            setShowHelp(false);
            setShowTour(true);
          }}
        />
      )}
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
