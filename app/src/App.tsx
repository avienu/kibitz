import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { parseFen } from "chessops/fen";
import { makeSan } from "chessops/san";
import { parseSquare, squareRank } from "chessops/util";
import AnnotatedMoves from "./AnnotatedMoves";
import Board, { type BoardMovable } from "./Board";
import DatabaseView from "./DatabaseView";
import PrepView from "./PrepView";
import {
  clampPly,
  gameFromSans,
  lastMoveAt,
  loadGame,
  numberedSans,
  type LoadedGame,
} from "./lib/game";
import {
  getGame,
  getGameTokens,
  explainPosition,
  updateGameTokens,
  type Explanation,
  type GameDetail,
} from "./lib/db";
import { shapesFromRecord } from "./lib/explainView";
import { insertVariation, type JsonToken } from "./lib/tokens";
import {
  formatScore,
  pvToSan,
  summarizeInfo,
  type EngineDone,
  type EngineInfo,
} from "./lib/engineView";
import {
  analyzePosition,
  getSavedEnginePath,
  getSavedNodes,
  onEngineDone,
  onEngineInfo,
  resolveEnginePath,
  saveEnginePath,
  saveNodes,
  stopAnalysis,
} from "./lib/engine";

const SAMPLE_PGN = `[Event "London"]
[Site "London ENG"]
[Date "1851.06.21"]
[White "Adolf Anderssen"]
[Black "Lionel Kieseritzky"]
[Result "1-0"]

1. e4 e5 2. f4 exf4 3. Bc4 Qh4+ 4. Kf1 b5 5. Bxb5 Nf6 6. Nf3 Qh6
7. d3 Nh5 8. Nh4 Qg5 9. Nf5 c6 10. g4 Nf6 11. Rg1 cxb5 12. h4 Qg6
13. h5 Qg5 14. Qf3 Ng8 15. Bxf4 Qf6 16. Nc3 Bc5 17. Nd5 Qxb2
18. Bd6 Bxg1 19. e5 Qxa1+ 20. Ke2 Na6 21. Nxg7+ Kd8 22. Qf6+ Nxf6
23. Be7# 1-0`;

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

type Mode = "analyze" | "database" | "prep";

/** Annotation-edit state for the currently loaded database game. */
interface AnnotState {
  gameId: number;
  startFen: string;
  tokens: JsonToken[];
  /** The last-persisted stream (dirty = tokens !== saved). */
  saved: JsonToken[];
}

/** A board-entered alternative to a mainline move, awaiting confirmation. */
interface PendingVariation {
  /** 1-based mainline ply the variation would replace. */
  ply: number;
  san: string;
  label: string;
}

export default function App() {
  const [mode, setMode] = useState<Mode>("analyze");
  const [pgnText, setPgnText] = useState("");
  const [game, setGame] = useState<LoadedGame | null>(null);
  const [ply, setPly] = useState(0);
  const [status, setStatus] = useState("Paste a PGN (or open a file) and press Load.");

  const [annot, setAnnot] = useState<AnnotState | null>(null);
  const [saving, setSaving] = useState(false);
  const [pendingVar, setPendingVar] = useState<PendingVariation | null>(null);

  const [explanation, setExplanation] = useState<Explanation | null>(null);
  const [explaining, setExplaining] = useState(false);

  const [enginePath, setEnginePath] = useState(getSavedEnginePath);
  const [resolvedPath, setResolvedPath] = useState<string>("");
  const [nodes, setNodes] = useState(getSavedNodes);
  const [analyzing, setAnalyzing] = useState(false);
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [done, setDone] = useState<EngineDone | null>(null);
  /** FEN the current/last analysis was started on (for POV-correct display). */
  const [analyzedFen, setAnalyzedFen] = useState<string>(START_FEN);

  const fileInputRef = useRef<HTMLInputElement | null>(null);
  /** Monotonic id so a slow get_game_tokens response can't attach to a
   * different game loaded in the meantime. */
  const tokenReqRef = useRef(0);

  const fen = game ? game.fens[ply] : START_FEN;
  const lastMove = game ? lastMoveAt(game, ply) : undefined;
  const moveList = useMemo(() => (game ? numberedSans(game) : []), [game]);

  // Engine event subscriptions (Tauri events from the Rust UCI manager).
  useEffect(() => {
    const unsubs: Array<() => void> = [];
    onEngineInfo((i) => setInfo(i)).then((u) => unsubs.push(u));
    onEngineDone((d) => {
      setDone(d);
      setAnalyzing(false);
    }).then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  }, []);

  // Show which engine binary would be used, whenever the override changes.
  useEffect(() => {
    let cancelled = false;
    resolveEnginePath(enginePath)
      .then((p) => !cancelled && setResolvedPath(p))
      .catch((e) => !cancelled && setResolvedPath(`unresolved: ${e}`));
    return () => {
      cancelled = true;
    };
  }, [enginePath]);

  // Position-bound overlays don't survive a position change.
  useEffect(() => {
    setExplanation(null);
    setPendingVar(null);
  }, [fen]);

  /** Install a freshly built game model and reset the stepper. */
  const applyGame = useCallback(
    (g: LoadedGame, label: string, warning?: string, atPly = 0) => {
      setGame(g);
      setPly(clampPly(atPly, g));
      setStatus(label + (warning ? ` ${warning}` : ""));
      if (analyzing) void stopAnalysis();
    },
    [analyzing],
  );

  const doLoad = useCallback(
    (text: string) => {
      const res = loadGame(text);
      if (!res.ok) {
        setStatus(res.error);
        return;
      }
      const w = res.game.headers["White"] ?? "?";
      const b = res.game.headers["Black"] ?? "?";
      tokenReqRef.current++; // invalidate any in-flight token fetch
      setAnnot(null);
      applyGame(res.game, `${w} — ${b}, ${res.game.sans.length} plies.`, res.warning);
    },
    [applyGame],
  );

  /** A game fetched from the database (Database tab / prep master game). */
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
      applyGame(
        res.game,
        `#${detail.id} ${detail.white} — ${detail.black}${elos}, ${detail.result}, ${res.game.sans.length} plies.`,
        res.warning,
        atPly,
      );
      const req = ++tokenReqRef.current;
      getGameTokens(detail.id)
        .then((gt) => {
          if (tokenReqRef.current !== req) return; // another game loaded since
          setAnnot({
            gameId: detail.id,
            startFen: gt.startFen,
            tokens: gt.tokens,
            saved: gt.tokens,
          });
        })
        .catch((e) => setStatus((s) => `${s} (annotations unavailable: ${e})`));
    },
    [applyGame],
  );

  /** Prep view: load a master game and jump to the prep position's ply. */
  const loadDbGameAt = useCallback(
    async (gameId: number, atPly: number) => {
      try {
        const detail = await getGame(gameId);
        loadDbGame(detail, atPly);
      } catch (e) {
        setStatus(String(e));
      }
    },
    [loadDbGame],
  );

  const step = useCallback(
    (delta: number) => {
      if (!game) return;
      setPly((p) => clampPly(p + delta, game));
      if (analyzing) void stopAnalysis();
    },
    [game, analyzing],
  );

  // Arrow-key navigation (ignored while typing in inputs/textareas).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "TEXTAREA" || t.tagName === "INPUT")) return;
      if (e.key === "ArrowRight") {
        e.preventDefault();
        step(1);
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        step(-1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [step]);

  const openFile = (f: File) => {
    f.text().then((text) => {
      setPgnText(text);
      doLoad(text);
    });
  };

  /** Board move input (enabled for annotatable database games): the
   * mainline move advances; any other legal move is offered as a variation. */
  const handleBoardMove = useCallback(
    (orig: string, dest: string) => {
      if (!game || !annot) return;
      const setup = parseFen(game.fens[ply]);
      if (setup.isErr) return;
      const p = Chess.fromSetup(setup.unwrap());
      if (p.isErr) return;
      const pos = p.unwrap();
      const from = parseSquare(orig);
      const to = parseSquare(dest);
      if (from === undefined || to === undefined) return;
      const promotion =
        pos.board.get(from)?.role === "pawn" && (squareRank(to) === 0 || squareRank(to) === 7)
          ? ("queen" as const)
          : undefined;
      // Normalize castling (king-two-squares vs king-onto-rook input forms).
      const move = normalizeMove(pos, { from, to, promotion });
      if (!pos.isLegal(move)) return;
      const san = makeSan(pos, move);
      if (ply < game.sans.length && san === game.sans[ply]) {
        step(1);
        return;
      }
      if (ply >= game.sans.length) {
        setStatus("End of the mainline — a board move can only vary an existing move.");
        return;
      }
      const num = pos.turn === "white" ? `${pos.fullmoves}.` : `${pos.fullmoves}...`;
      setPendingVar({ ply: ply + 1, san, label: `${num} ${san}` });
    },
    [game, annot, ply, step],
  );

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
      const detail = await getGame(annot.gameId);
      loadDbGame(detail, ply);
      setStatus(`Annotations saved for game #${annot.gameId}.`);
    } catch (e) {
      setStatus(`Save failed: ${e}`);
    } finally {
      setSaving(false);
    }
  }, [annot, ply, loadDbGame]);

  const revertAnnotations = useCallback(() => {
    setAnnot((a) => (a ? { ...a, tokens: a.saved } : a));
    setPendingVar(null);
  }, []);

  const doExplain = useCallback(async () => {
    setExplaining(true);
    try {
      setExplanation(await explainPosition(fen));
    } catch (e) {
      setStatus(`Explain failed: ${e}`);
    } finally {
      setExplaining(false);
    }
  }, [fen]);

  const shapes = useMemo(
    () => (explanation ? shapesFromRecord(explanation.record) : undefined),
    [explanation],
  );

  const startAnalysis = async () => {
    setInfo(null);
    setDone(null);
    setAnalyzedFen(fen);
    setAnalyzing(true);
    try {
      await analyzePosition(fen, nodes, enginePath);
    } catch (e) {
      setAnalyzing(false);
      setDone({ error: String(e) });
    }
  };

  const evalStr = info ? formatScore(info, analyzedFen) : "—";
  const pvSan = info?.pv ? pvToSan(analyzedFen, info.pv) : "";
  const bestmoveSan =
    done?.bestmove && !done.error ? pvToSan(analyzedFen, [done.bestmove]) : undefined;

  const annotDirty = annot !== null && annot.tokens !== annot.saved;

  return (
    <div className="layout">
      <div className="left">
        <Board fen={fen} lastMove={lastMove} movable={movable} shapes={shapes} />
        <div className="nav">
          <button onClick={() => setPly(0)} disabled={!game}>
            |&lt;
          </button>
          <button onClick={() => step(-1)} disabled={!game || ply === 0}>
            ◀ Prev
          </button>
          <button onClick={() => step(1)} disabled={!game || ply >= (game?.sans.length ?? 0)}>
            Next ▶
          </button>
          <button onClick={() => game && setPly(game.sans.length)} disabled={!game}>
            &gt;|
          </button>
          <span className="ply">{game ? `ply ${ply}/${game.sans.length}` : "no game"}</span>
        </div>
        {pendingVar && game && (
          <div className="var-offer">
            <span>
              Add {pendingVar.label} as a variation of {game.sans[pendingVar.ply - 1]}?
            </span>
            <button onClick={acceptPendingVar}>Add as variation</button>
            <button onClick={() => setPendingVar(null)}>Dismiss</button>
          </div>
        )}
        <div className="status">{status}</div>

        <div className="engine">
          <h3>Engine</h3>
          <div className="engine-row">
            <button onClick={startAnalysis} disabled={analyzing}>
              Analyze
            </button>
            <button onClick={() => void stopAnalysis()} disabled={!analyzing}>
              Stop
            </button>
            <label>
              nodes{" "}
              <input
                type="number"
                min={1}
                value={nodes}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  if (Number.isFinite(n) && n > 0) {
                    setNodes(n);
                    saveNodes(n);
                  }
                }}
              />
            </label>
          </div>
          <div className="engine-row">
            <label>
              engine path (optional override){" "}
              <input
                type="text"
                placeholder="resolved automatically if empty"
                value={enginePath}
                onChange={(e) => {
                  setEnginePath(e.target.value);
                  saveEnginePath(e.target.value);
                }}
              />
            </label>
          </div>
          <div className="engine-resolved">using: {resolvedPath || "…"}</div>
          <div className="eval">
            <span className="score">{evalStr}</span>
            {info && <span className="detail">{summarizeInfo(info, analyzedFen)}</span>}
            {analyzing && <span className="running">searching…</span>}
          </div>
          {pvSan && <div className="pv">PV: {pvSan}</div>}
          {bestmoveSan && <div className="best">bestmove: {bestmoveSan}</div>}
          {done?.error && <div className="error">engine error: {done.error}</div>}
        </div>

        <div className="explain">
          <h3>Explain (static, no engine)</h3>
          <div className="engine-row">
            <button onClick={() => void doExplain()} disabled={explaining}>
              {explaining ? "Explaining…" : "Explain position"}
            </button>
            {explanation && <button onClick={() => setExplanation(null)}>Clear</button>}
          </div>
          {explanation && (
            <div className="explain-prose">
              {explanation.prose.split("\n\n").map((p, i) => (
                <p key={i}>{p}</p>
              ))}
              <div className="explain-legend">
                <span className="lg lg-red">alert targets</span>
                <span className="lg lg-orange">attackers</span>
                <span className="lg lg-green">imbalance evidence</span>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className={mode === "analyze" ? "right" : "right db"}>
        <div className="tabs">
          <button className={mode === "analyze" ? "cur" : ""} onClick={() => setMode("analyze")}>
            Analyze
          </button>
          <button className={mode === "database" ? "cur" : ""} onClick={() => setMode("database")}>
            Database
          </button>
          <button className={mode === "prep" ? "cur" : ""} onClick={() => setMode("prep")}>
            Prep
          </button>
        </div>

        {mode === "analyze" && (
          <>
            <h3>PGN</h3>
            <textarea
              value={pgnText}
              onChange={(e) => setPgnText(e.target.value)}
              placeholder="Paste PGN here…"
              spellCheck={false}
            />
            <div className="pgn-buttons">
              <button onClick={() => doLoad(pgnText)}>Load</button>
              <button onClick={() => fileInputRef.current?.click()}>Open file…</button>
              <button
                onClick={() => {
                  setPgnText(SAMPLE_PGN);
                  doLoad(SAMPLE_PGN);
                }}
              >
                Sample game
              </button>
              <input
                ref={fileInputRef}
                type="file"
                accept=".pgn,.txt"
                style={{ display: "none" }}
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) openFile(f);
                  e.target.value = "";
                }}
              />
            </div>
          </>
        )}
        {mode === "database" && (
          <DatabaseView
            currentFen={fen}
            game={game}
            ply={ply}
            onLoadGame={loadDbGame}
            onAdvance={() => step(1)}
          />
        )}
        {mode === "prep" && <PrepView onLoadGameAt={loadDbGameAt} />}

        {game &&
          (annot ? (
            <AnnotatedMoves
              startFen={annot.startFen}
              tokens={annot.tokens}
              currentPly={ply}
              dirty={annotDirty}
              saving={saving}
              onSelectPly={(p) => {
                setPly(clampPly(p, game));
                if (analyzing) void stopAnalysis();
              }}
              onChange={(tokens) => setAnnot((a) => (a ? { ...a, tokens } : a))}
              onSave={() => void saveAnnotations()}
              onRevert={revertAnnotations}
            />
          ) : (
            <ol className="moves">
              {moveList.map((m, i) => (
                <li key={i}>
                  <button className={i + 1 === ply ? "cur" : ""} onClick={() => setPly(i + 1)}>
                    {m}
                  </button>
                </li>
              ))}
            </ol>
          ))}
      </div>
    </div>
  );
}
