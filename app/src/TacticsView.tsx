/**
 * Tactics tab (ROADMAP Phase 5): rated drill, motif filter,
 * weakness-weighted drill (profile-driven, with the "why this puzzle"
 * explanation), Woodpecker cycles and the Heisman speed drill.
 *
 * Self-contained: owns its board (the puzzle position is independent of
 * the game loaded in the left pane). No engine anywhere in this flow.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { parseFen } from "chessops/fen";
import { makeUci, parseSquare, squareRank } from "chessops/util";
import Board, { type BoardMovable } from "./Board";
import { usePromotionPicker } from "./PromotionPicker";
import type { PlayerProfile } from "./lib/db";
import type { PromoRole } from "./lib/promotion";
import {
  buildPuzzleModel,
  createWoodpeckerSet,
  cycleStats,
  finishCycle,
  formatClock,
  importPuzzles,
  isSolverMove,
  motifWeightsFromProfile,
  nextPuzzle,
  recordAttempt,
  startCycle,
  tacticsState,
  verifyMove,
  woodpeckerPuzzles,
  woodpeckerSets,
  type CycleStats,
  type DrillMode,
  type PuzzleModel,
  type PuzzleRow,
  type ServedPuzzle,
  type TacticsState,
  type WoodpeckerSet,
} from "./lib/tactics";

type Phase = "idle" | "solving" | "solved" | "failed";

interface SpeedSession {
  attempts: number;
  solved: number;
  totalMs: number;
}

const DEFAULT_CSV = "testdata/corpus/lichess_db_puzzle.csv";

export default function TacticsView({ profile }: { profile: PlayerProfile | null }) {
  const [st, setSt] = useState<TacticsState | null>(null);
  const [status, setStatus] = useState("Pick a drill mode and press Next puzzle.");
  const [mode, setMode] = useState<DrillMode>("rated");
  const [theme, setTheme] = useState("");

  const [served, setServed] = useState<ServedPuzzle | null>(null);
  const [model, setModel] = useState<PuzzleModel | null>(null);
  const [lineIdx, setLineIdx] = useState(0);
  const [phase, setPhase] = useState<Phase>("idle");
  const [outcomeText, setOutcomeText] = useState("");
  const startedAtRef = useRef(0);
  const [, setTick] = useState(0); // re-render pulse for the clock
  const busyRef = useRef(false); // one verify in flight at a time

  const [importPath, setImportPath] = useState(DEFAULT_CSV);
  const [importMinPop, setImportMinPop] = useState("50");
  const [importing, setImporting] = useState(false);

  const [sets, setSets] = useState<WoodpeckerSet[]>([]);
  const [setName, setSetName] = useState("");
  const [setSize, setSetSize] = useState("50");
  const [statsFor, setStatsFor] = useState<{ set: WoodpeckerSet; stats: CycleStats[] } | null>(
    null,
  );
  const [cycle, setCycle] = useState<{
    set: WoodpeckerSet;
    cycleId: number;
    queue: PuzzleRow[];
    idx: number;
    solved: number;
    totalMs: number;
  } | null>(null);

  const [speed, setSpeed] = useState<SpeedSession>({ attempts: 0, solved: 0, totalMs: 0 });

  const loadState = useCallback(() => {
    tacticsState()
      .then(setSt)
      .catch((e) => setStatus(`Tactics unavailable: ${e}`));
    woodpeckerSets()
      .then(setSets)
      .catch(() => {});
  }, []);
  useEffect(loadState, [loadState]);

  // Clock pulse while solving.
  useEffect(() => {
    if (phase !== "solving") return;
    const t = setInterval(() => setTick((n) => n + 1), 500);
    return () => clearInterval(t);
  }, [phase]);

  const fen = model ? model.fens[Math.min(lineIdx, model.fens.length - 1)] : undefined;
  const lastMove =
    model && lineIdx > 0 ? model.lastMoves[Math.min(lineIdx, model.lastMoves.length) - 1] : undefined;

  /** Install a puzzle and play the opponent's setup move after a beat. */
  const install = useCallback((sp: ServedPuzzle) => {
    const m = buildPuzzleModel(sp.puzzle.fen, sp.puzzle.moves);
    if (!m) {
      setStatus(`Puzzle ${sp.puzzle.lichessId} failed to replay (corrupt data) — skipped.`);
      return;
    }
    setServed(sp);
    setModel(m);
    setLineIdx(0);
    setPhase("solving");
    setOutcomeText("");
    setTimeout(() => {
      setLineIdx(1);
      startedAtRef.current = Date.now();
    }, 450);
  }, []);

  const serve = useCallback(async () => {
    try {
      if (mode === "woodpecker") {
        if (!cycle) {
          setStatus("Start a Woodpecker cycle below first.");
          return;
        }
        if (cycle.idx >= cycle.queue.length) return;
        install({ puzzle: cycle.queue[cycle.idx], matchedThemes: [], allowed: 0, missed: 0 });
        return;
      }
      if (mode === "motif" && !theme) {
        setStatus("Pick a theme for the motif drill.");
        return;
      }
      if (mode === "weakness" && !profile) {
        setStatus("Weakness drill needs your profile — build it in the Player Profile view first.");
        return;
      }
      const sp = await nextPuzzle(
        mode as "rated" | "motif" | "weakness" | "speed",
        mode === "motif" ? theme : undefined,
        mode === "weakness" && profile ? motifWeightsFromProfile(profile) : undefined,
      );
      if (!sp) {
        setStatus("No unsolved puzzles in range — import more or widen your filters.");
        return;
      }
      install(sp);
    } catch (e) {
      setStatus(String(e));
    }
  }, [mode, theme, profile, cycle, install]);

  /** Record the finished attempt and update every dependent display. */
  const finish = useCallback(
    async (solved: boolean, note: string) => {
      if (!served) return;
      const timeMs = Math.max(1, Date.now() - startedAtRef.current);
      setPhase(solved ? "solved" : "failed");
      try {
        const out = await recordAttempt(
          served.puzzle.id,
          solved,
          timeMs,
          mode,
          cycle?.cycleId ?? undefined,
        );
        const delta = out.ratingAfter - out.ratingBefore;
        const ratingNote =
          Math.abs(delta) >= 0.05
            ? ` Rating ${out.ratingBefore.toFixed(0)} → ${out.ratingAfter.toFixed(0)} (${
                delta >= 0 ? "+" : ""
              }${delta.toFixed(1)}).`
            : "";
        setOutcomeText(`${note} ${formatClock(timeMs)}.${ratingNote}`);
        loadState();
      } catch (e) {
        setOutcomeText(`${note} (recording failed: ${e})`);
      }
      if (mode === "speed") {
        setSpeed((s) => ({
          attempts: s.attempts + 1,
          solved: s.solved + (solved ? 1 : 0),
          totalMs: s.totalMs + timeMs,
        }));
      }
      if (mode === "woodpecker" && cycle) {
        const idx = cycle.idx + 1;
        const next = {
          ...cycle,
          idx,
          solved: cycle.solved + (solved ? 1 : 0),
          totalMs: cycle.totalMs + timeMs,
        };
        setCycle(next);
        if (idx >= cycle.queue.length) {
          try {
            await finishCycle(cycle.cycleId);
            const stats = await cycleStats(cycle.set.id);
            setStatsFor({ set: cycle.set, stats });
            setCycle(null);
            setStatus(
              `Cycle finished: ${next.solved}/${next.queue.length} solved in ${formatClock(next.totalMs)}.`,
            );
            loadState();
          } catch (e) {
            setStatus(String(e));
          }
        }
      }
    },
    [served, mode, cycle, loadState],
  );

  // Promotion picker (run-6 item 3): the drag defers to the overlay, which
  // re-invokes the handler with the chosen role — underpromotions included.
  const moveHandlerRef = useRef<(orig: string, dest: string, promoRole?: PromoRole) => void>(
    () => {},
  );
  const promo = usePromotionPicker((orig, dest, role) => moveHandlerRef.current(orig, dest, role));

  const onBoardMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!served || !model || phase !== "solving" || !isSolverMove(lineIdx)) return;
      if (busyRef.current) return;
      const cur = model.fens[lineIdx];
      if (!promoRole && promo.guard(cur, orig, dest)) return;
      const setup = parseFen(cur);
      if (setup.isErr) return;
      const p = Chess.fromSetup(setup.unwrap());
      if (p.isErr) return;
      const pos = p.unwrap();
      const from = parseSquare(orig);
      const to = parseSquare(dest);
      if (from === undefined || to === undefined) return;
      const promotion =
        pos.board.get(from)?.role === "pawn" && (squareRank(to) === 0 || squareRank(to) === 7)
          ? (promoRole ?? "queen")
          : undefined;
      const move = normalizeMove(pos, { from, to, promotion });
      if (!pos.isLegal(move)) return;
      const played = makeUci(move);
      const expected = served.puzzle.moves[lineIdx];
      busyRef.current = true;
      verifyMove(cur, expected, played)
        .then((verdict) => {
          if (verdict === "wrong") {
            void finish(
              false,
              `Wrong — the answer was ${model.sans[lineIdx]}. Full solution: ${solutionText(model)}.`,
            );
            return;
          }
          const afterUser = lineIdx + 1;
          if (verdict === "correctAltMate") {
            setLineIdx(afterUser); // show the stored mate; theirs also mates
            void finish(true, "Checkmate — your alternate mate works too. Solved in");
            return;
          }
          setLineIdx(afterUser);
          if (afterUser >= served.puzzle.moves.length) {
            void finish(true, "Solved in");
          } else {
            // Opponent replies after a beat.
            setTimeout(() => setLineIdx(afterUser + 1), 350);
          }
        })
        .catch((e) => setStatus(String(e)))
        .finally(() => {
          busyRef.current = false;
        });
    },
    [served, model, phase, lineIdx, finish, promo],
  );
  moveHandlerRef.current = onBoardMove;

  const movable = useMemo((): BoardMovable | undefined => {
    if (!model || phase !== "solving" || !isSolverMove(lineIdx)) return undefined;
    const setup = parseFen(model.fens[lineIdx]);
    if (setup.isErr) return undefined;
    const p = Chess.fromSetup(setup.unwrap());
    if (p.isErr) return undefined;
    const pos = p.unwrap();
    if (pos.turn !== model.solverColor) return undefined;
    return { color: pos.turn, dests: chessgroundDests(pos), onMove: onBoardMove };
  }, [model, phase, lineIdx, onBoardMove]);

  const doImport = useCallback(async () => {
    setImporting(true);
    setStatus("Importing puzzles — the full dump takes a few minutes…");
    try {
      const minPop = importMinPop.trim() === "" ? undefined : parseInt(importMinPop, 10);
      const r = await importPuzzles(importPath, Number.isFinite(minPop) ? minPop : undefined);
      setStatus(
        `Imported ${r.imported} puzzles (${r.duplicatesSkipped} duplicates, ` +
          `${r.filteredOut} filtered, ${r.malformed} malformed) in ${formatClock(r.elapsedMs)}.`,
      );
      loadState();
    } catch (e) {
      setStatus(`Import failed: ${e}`);
    } finally {
      setImporting(false);
    }
  }, [importPath, importMinPop, loadState]);

  const doCreateSet = useCallback(async () => {
    const size = parseInt(setSize, 10);
    if (!setName.trim() || !Number.isFinite(size) || size <= 0) {
      setStatus("Give the Woodpecker set a name and a positive size.");
      return;
    }
    try {
      await createWoodpeckerSet(setName.trim(), size);
      setSetName("");
      loadState();
    } catch (e) {
      setStatus(String(e));
    }
  }, [setName, setSize, loadState]);

  const doStartCycle = useCallback(
    async (set: WoodpeckerSet) => {
      try {
        const [cycleId, queue] = await Promise.all([startCycle(set.id), woodpeckerPuzzles(set.id)]);
        setMode("woodpecker");
        setStatsFor(null);
        setCycle({ set, cycleId, queue, idx: 0, solved: 0, totalMs: 0 });
        const first = queue[0];
        if (first) install({ puzzle: first, matchedThemes: [], allowed: 0, missed: 0 });
        setStatus(`Cycle started on "${set.name}" (${queue.length} puzzles).`);
      } catch (e) {
        setStatus(String(e));
      }
    },
    [install],
  );

  const showStats = useCallback(async (set: WoodpeckerSet) => {
    try {
      setStatsFor({ set, stats: await cycleStats(set.id) });
    } catch (e) {
      setStatus(String(e));
    }
  }, []);

  const elapsed = phase === "solving" && lineIdx > 0 ? Date.now() - startedAtRef.current : 0;
  const finished = phase === "solved" || phase === "failed";
  const nextLabel = mode === "woodpecker" && cycle ? "Next in cycle" : "Next puzzle";
  const canNext =
    phase !== "solving" && (mode !== "woodpecker" || (cycle !== null && cycle.idx < cycle.queue.length));

  return (
    <div className="tactics">
      <div className="db-summary">
        {st
          ? `Tactics rating ${st.rating.toFixed(0)} (${st.attempts} rated attempts) — ${st.puzzles} puzzles imported.`
          : "Open a database (Database tab) to train tactics."}
      </div>

      <div className="db-section">
        <h3>Drill</h3>
        <div className="engine-row">
          <label>
            mode{" "}
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value as DrillMode)}
              disabled={phase === "solving"}
            >
              <option value="rated">Rated (±100 of your rating)</option>
              <option value="motif">Motif filter</option>
              <option value="weakness">Weakness-weighted (from your profile)</option>
              <option value="woodpecker">Woodpecker cycle</option>
              <option value="speed">Speed (easy, against the clock)</option>
            </select>
          </label>
          {mode === "motif" && (
            <label>
              theme{" "}
              <select value={theme} onChange={(e) => setTheme(e.target.value)}>
                <option value="">— pick —</option>
                {(st?.themes ?? []).slice(0, 60).map((t) => (
                  <option key={t.theme} value={t.theme}>
                    {t.theme} ({t.puzzles})
                  </option>
                ))}
              </select>
            </label>
          )}
          <button onClick={() => void serve()} disabled={!st || st.puzzles === 0 || !canNext}>
            {nextLabel}
          </button>
          {phase === "solving" && lineIdx > 0 && (
            <button onClick={() => model && void finish(false, `Gave up — solution: ${solutionText(model)}. After`)}>
              Give up
            </button>
          )}
        </div>

        {served && model && (
          <>
            <div className="tactics-board">
              <Board
                fen={fen ?? ""}
                lastMove={lastMove}
                movable={movable}
                orientation={model.solverColor}
                size={376}
              />
              {promo.element}
            </div>
            <div className="tactics-line">
              <span className={`tactics-phase ${phase}`}>
                {phase === "solving" &&
                  (lineIdx === 0
                    ? "…"
                    : `${model.solverColor === "white" ? "White" : "Black"} to move — ${formatClock(elapsed)}`)}
                {phase === "solved" && "Solved ✓"}
                {phase === "failed" && "Failed ✗"}
              </span>
              <span className="tactics-meta">
                #{served.puzzle.lichessId} · rated {served.puzzle.rating}
                {finished ? ` · ${served.puzzle.themes.join(", ")}` : ""}
              </span>
            </div>
            {finished && (
              <div className="tactics-nav">
                <button onClick={() => setLineIdx((i) => Math.max(0, i - 1))} disabled={lineIdx === 0}>
                  ◀
                </button>
                <button
                  onClick={() => setLineIdx((i) => Math.min(model.fens.length - 1, i + 1))}
                  disabled={lineIdx >= model.fens.length - 1}
                >
                  ▶
                </button>
                <span>replay the solution</span>
              </div>
            )}
            {outcomeText && <div className="tactics-outcome">{outcomeText}</div>}
            {served.reason && (
              <div className="tactics-reason">Why this puzzle: {served.reason}</div>
            )}
          </>
        )}

        {mode === "speed" && speed.attempts > 0 && (
          <div className="tactics-session">
            Speed session: {speed.solved}/{speed.attempts} solved, avg{" "}
            {formatClock(speed.totalMs / speed.attempts)} per puzzle.
          </div>
        )}
        {mode === "woodpecker" && cycle && (
          <div className="tactics-session">
            Cycle {`"${cycle.set.name}"`}: puzzle {Math.min(cycle.idx + 1, cycle.queue.length)}/
            {cycle.queue.length}, {cycle.solved} solved, {formatClock(cycle.totalMs)} total.
          </div>
        )}
      </div>

      <div className="db-section">
        <h3>Woodpecker sets</h3>
        <div className="engine-row">
          <input
            type="text"
            placeholder="set name"
            value={setName}
            onChange={(e) => setSetName(e.target.value)}
          />
          <input
            type="number"
            min={1}
            value={setSize}
            onChange={(e) => setSetSize(e.target.value)}
            style={{ width: "5em" }}
          />
          <button onClick={() => void doCreateSet()} disabled={!st || st.puzzles === 0}>
            Create set
          </button>
        </div>
        {sets.length > 0 && (
          <table className="tactics-table">
            <thead>
              <tr>
                <th>set</th>
                <th>puzzles</th>
                <th>cycles</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {sets.map((s) => (
                <tr key={s.id}>
                  <td>{s.name}</td>
                  <td>{s.size}</td>
                  <td>{s.cycles}</td>
                  <td>
                    <button onClick={() => void doStartCycle(s)} disabled={cycle !== null}>
                      Start cycle
                    </button>{" "}
                    <button onClick={() => void showStats(s)}>Stats</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {statsFor && statsFor.stats.length > 0 && (
          <table className="tactics-table">
            <thead>
              <tr>
                <th>cycle of {`"${statsFor.set.name}"`}</th>
                <th>attempts</th>
                <th>solved</th>
                <th>accuracy</th>
                <th>total</th>
                <th>avg</th>
              </tr>
            </thead>
            <tbody>
              {statsFor.stats.map((c) => (
                <tr key={c.cycleId}>
                  <td>#{c.cycleNo}</td>
                  <td>{c.attempts}</td>
                  <td>{c.solved}</td>
                  <td>{c.accuracyPct.toFixed(1)}%</td>
                  <td>{formatClock(c.totalTimeMs)}</td>
                  <td>{formatClock(c.avgTimeMs)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="db-section">
        <h3>Import puzzles</h3>
        <div className="engine-row">
          <input
            type="text"
            value={importPath}
            onChange={(e) => setImportPath(e.target.value)}
            placeholder={DEFAULT_CSV}
          />
          <label>
            min popularity{" "}
            <input
              type="number"
              value={importMinPop}
              onChange={(e) => setImportMinPop(e.target.value)}
              style={{ width: "5em" }}
            />
          </label>
          <button onClick={() => void doImport()} disabled={importing}>
            {importing ? "Importing…" : "Import CSV"}
          </button>
        </div>
        <div className="tactics-hint">
          Lichess puzzle database (CC0) — download lichess_db_puzzle.csv from
          database.lichess.org.
        </div>
      </div>

      <div className="status">{status}</div>
    </div>
  );
}

function solutionText(model: PuzzleModel): string {
  return model.sans.slice(1).join(" ");
}
