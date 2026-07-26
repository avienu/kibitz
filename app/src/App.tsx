import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Board from "./Board";
import DatabaseView from "./DatabaseView";
import {
  clampPly,
  gameFromSans,
  lastMoveAt,
  loadGame,
  numberedSans,
  type LoadedGame,
} from "./lib/game";
import type { GameDetail } from "./lib/db";
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

type Mode = "analyze" | "database";

export default function App() {
  const [mode, setMode] = useState<Mode>("analyze");
  const [pgnText, setPgnText] = useState("");
  const [game, setGame] = useState<LoadedGame | null>(null);
  const [ply, setPly] = useState(0);
  const [status, setStatus] = useState("Paste a PGN (or open a file) and press Load.");

  const [enginePath, setEnginePath] = useState(getSavedEnginePath);
  const [resolvedPath, setResolvedPath] = useState<string>("");
  const [nodes, setNodes] = useState(getSavedNodes);
  const [analyzing, setAnalyzing] = useState(false);
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [done, setDone] = useState<EngineDone | null>(null);
  /** FEN the current/last analysis was started on (for POV-correct display). */
  const [analyzedFen, setAnalyzedFen] = useState<string>(START_FEN);

  const fileInputRef = useRef<HTMLInputElement | null>(null);

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

  /** Install a freshly built game model and reset the stepper. */
  const applyGame = useCallback(
    (g: LoadedGame, label: string, warning?: string) => {
      setGame(g);
      setPly(0);
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
      applyGame(res.game, `${w} — ${b}, ${res.game.sans.length} plies.`, res.warning);
    },
    [applyGame],
  );

  /** A game fetched from the database (Database tab row click / load). */
  const loadDbGame = useCallback(
    (detail: GameDetail) => {
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
      applyGame(
        res.game,
        `#${detail.id} ${detail.white} — ${detail.black}${elos}, ${detail.result}, ${res.game.sans.length} plies.`,
        res.warning,
      );
    },
    [applyGame],
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

  return (
    <div className="layout">
      <div className="left">
        <Board fen={fen} lastMove={lastMove} />
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
      </div>

      <div className={mode === "database" ? "right db" : "right"}>
        <div className="tabs">
          <button className={mode === "analyze" ? "cur" : ""} onClick={() => setMode("analyze")}>
            Analyze
          </button>
          <button className={mode === "database" ? "cur" : ""} onClick={() => setMode("database")}>
            Database
          </button>
        </div>

        {mode === "analyze" ? (
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
        ) : (
          <DatabaseView
            currentFen={fen}
            game={game}
            ply={ply}
            onLoadGame={loadDbGame}
            onAdvance={() => step(1)}
          />
        )}

        {game && (
          <ol className="moves">
            {moveList.map((m, i) => (
              <li key={i}>
                <button className={i + 1 === ply ? "cur" : ""} onClick={() => setPly(i + 1)}>
                  {m}
                </button>
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}
