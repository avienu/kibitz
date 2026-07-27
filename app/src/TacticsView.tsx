/**
 * Tactics — round-2 build-out (design/handoff-2 §Screen: Tactics).
 *
 * `230px mode column | board column (--desk) | 400px reasoning aside`.
 * Five drill modes (weakness-targeted is the default), a Woodpecker cycle
 * panel of baseline bars, a 640px meta row (side to move · streak ·
 * rating · clock in timed modes only), the board at 560 flipped to the
 * solver's side with NO evidence overlays ever, and the WHY THIS PUZZLE
 * aside with the coach/neutral voice shared with Explain.
 *
 * Keyboard: H hint · S skip · G give up · ⏎ next (never inside inputs).
 * "Train this weakness" seeding: `seedClaim` (App routes params.claim)
 * parses to a motif and restricts the weakness selector's weights to it.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Chess, normalizeMove } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { parseFen } from "chessops/fen";
import { makeUci, parseSquare, squareRank } from "chessops/util";
import Board, { type BoardMovable } from "./Board";
import BaselineBar from "./components/BaselineBar";
import ScreenHeader from "./shell/ScreenHeader";
import { usePromotionPicker } from "./PromotionPicker";
import type { NarrationVoice, PlayerProfile } from "./lib/db";
import type { BoardTreatment } from "./lib/evidence";
import { isEditableTarget, type EditableTargetLike } from "./lib/gameView";
import type { PromoRole } from "./lib/promotion";
import {
  buildPuzzleModel,
  createWoodpeckerSet,
  cycleStats,
  finishCycle,
  formatClock,
  importPuzzles,
  isSolverMove,
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
import {
  MODE_DEFS,
  isTimedMode,
  modeBadge,
  motifFact,
  seedMotifFromClaim,
  sourceFact,
  tacticsKeyAction,
  weaknessWeights,
  whyText,
} from "./lib/tacticsView";

type Phase = "idle" | "solving" | "solved" | "failed";

const DEFAULT_CSV = "testdata/corpus/lichess_db_puzzle.csv";

interface TacticsViewProps {
  profile: PlayerProfile | null;
  /** "Train this weakness" seed (ViewParams.claim routed by App):
   * "motif:<Kind>:missed|allowed" → weakness mode with that motif
   * emphasized (the weights array is the selector's motif hint). */
  seedClaim?: string | null;
  /** App-level narration voice — the same state Explain uses. */
  voice: NarrationVoice;
  onVoice: (v: NarrationVoice) => void;
  treatment?: BoardTreatment;
}

export default function TacticsView({
  profile,
  seedClaim,
  voice,
  onVoice,
  treatment = "walnut",
}: TacticsViewProps) {
  const initialSeed = seedMotifFromClaim(seedClaim);
  const [st, setSt] = useState<TacticsState | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  // Weakness-targeted is the design default; a seed also lands there.
  const [mode, setMode] = useState<DrillMode>("weakness");
  const [seededMotif, setSeededMotif] = useState<string | null>(initialSeed);
  const [theme, setTheme] = useState("");

  const [served, setServed] = useState<ServedPuzzle | null>(null);
  const [model, setModel] = useState<PuzzleModel | null>(null);
  const [lineIdx, setLineIdx] = useState(0);
  const [phase, setPhase] = useState<Phase>("idle");
  const [outcomeText, setOutcomeText] = useState("");
  const [hintSquare, setHintSquare] = useState<string | null>(null);
  const startedAtRef = useRef(0);
  const [, setTick] = useState(0); // re-render pulse for the clock
  const busyRef = useRef(false); // one verify in flight at a time

  const [streak, setStreak] = useState(0);
  const [session, setSession] = useState({ attempts: 0, solved: 0 });
  const sessionStartRatingRef = useRef<number | null>(null);

  const [sets, setSets] = useState<WoodpeckerSet[]>([]);
  const [setStats, setSetStats] = useState<CycleStats[]>([]);
  const [setName, setSetName] = useState("");
  const [setSize, setSetSize] = useState("50");
  const [cycle, setCycle] = useState<{
    set: WoodpeckerSet;
    cycleId: number;
    queue: PuzzleRow[];
    idx: number;
    solved: number;
    totalMs: number;
  } | null>(null);

  const [importPath, setImportPath] = useState(DEFAULT_CSV);
  const [importing, setImporting] = useState(false);

  // Late seed navigation (Profile → Train this weakness while mounted).
  useEffect(() => {
    const m = seedMotifFromClaim(seedClaim);
    if (m) {
      setSeededMotif(m);
      setMode("weakness");
    }
  }, [seedClaim]);

  const loadState = useCallback(() => {
    tacticsState()
      .then((s) => {
        setSt(s);
        if (sessionStartRatingRef.current === null) sessionStartRatingRef.current = s.rating;
      })
      .catch((e) => setStatus(`Tactics unavailable: ${e}`));
    woodpeckerSets()
      .then((ws) => {
        setSets(ws);
        const latest = ws[ws.length - 1];
        if (latest) {
          cycleStats(latest.id)
            .then(setSetStats)
            .catch(() => setSetStats([]));
        } else {
          setSetStats([]);
        }
      })
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
    setHintSquare(null);
    setTimeout(() => {
      setLineIdx(1);
      startedAtRef.current = Date.now();
    }, 450);
  }, []);

  const serve = useCallback(async () => {
    try {
      if (mode === "woodpecker") {
        if (!cycle) {
          setStatus("Start a Woodpecker cycle in the panel on the left first.");
          return;
        }
        if (cycle.idx >= cycle.queue.length) return;
        install({ puzzle: cycle.queue[cycle.idx], matchedThemes: [], allowed: 0, missed: 0 });
        return;
      }
      if (mode === "motif" && !theme) {
        setStatus("Pick a motif theme in the mode column first.");
        return;
      }
      if (mode === "weakness" && !profile && !seededMotif) {
        setStatus("Weakness drill needs your profile — build it on the Profile screen first.");
        return;
      }
      const sp = await nextPuzzle(
        mode as "rated" | "motif" | "weakness" | "speed",
        mode === "motif" ? theme : undefined,
        mode === "weakness" ? weaknessWeights(profile, seededMotif) : undefined,
      );
      if (!sp) {
        setStatus("No unsolved puzzles in range — import more or widen your filters.");
        return;
      }
      setStatus(null);
      install(sp);
    } catch (e) {
      setStatus(String(e));
    }
  }, [mode, theme, profile, seededMotif, cycle, install]);

  /** Record the finished attempt and update every dependent display. */
  const finish = useCallback(
    async (solved: boolean, note: string) => {
      if (!served) return;
      const timeMs = Math.max(1, Date.now() - startedAtRef.current);
      setPhase(solved ? "solved" : "failed");
      setHintSquare(null);
      setStreak((n) => (solved ? n + 1 : 0));
      setSession((s) => ({ attempts: s.attempts + 1, solved: s.solved + (solved ? 1 : 0) }));
      try {
        await recordAttempt(served.puzzle.id, solved, timeMs, mode, cycle?.cycleId ?? undefined);
        setOutcomeText(`${note} ${formatClock(timeMs)}.`);
        loadState();
      } catch (e) {
        setOutcomeText(`${note} (recording failed: ${e})`);
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

  // Promotion picker: the drag defers to the overlay, which re-invokes the
  // handler with the chosen role — underpromotions included.
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
      setHintSquare(null);
      verifyMove(cur, expected, played)
        .then((verdict) => {
          if (verdict === "wrong") {
            void finish(
              false,
              `Wrong — the answer was ${model.sans[lineIdx]}. Full solution: ${solutionText(model)}. After`,
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

  /* ---- controls ---- */
  const solvingNow = phase === "solving" && lineIdx > 0;
  const finished = phase === "solved" || phase === "failed";
  const canNext =
    phase !== "solving" &&
    (mode !== "woodpecker" || (cycle !== null && cycle.idx < cycle.queue.length));

  const doHint = useCallback(() => {
    if (!served || !model || !solvingNow || !isSolverMove(lineIdx)) return;
    const expected = served.puzzle.moves[lineIdx];
    setHintSquare(expected.slice(0, 2));
  }, [served, model, solvingNow, lineIdx]);

  const doSkip = useCallback(() => {
    if (!model || !solvingNow) return;
    void finish(false, `Skipped — solution: ${solutionText(model)}. After`);
  }, [model, solvingNow, finish]);

  const doGiveUp = useCallback(() => {
    if (!model || !solvingNow) return;
    setLineIdx(model.fens.length - 1);
    void finish(false, `Gave up — solution: ${solutionText(model)}. After`);
  }, [model, solvingNow, finish]);

  /* ---- keyboard (H / S / G / ⏎; never inside inputs) ---- */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (promo.active) return;
      const act = tacticsKeyAction(e.key, isEditableTarget(e.target as EditableTargetLike | null));
      if (!act) return;
      e.preventDefault();
      switch (act) {
        case "hint":
          doHint();
          break;
        case "skip":
          doSkip();
          break;
        case "giveup":
          doGiveUp();
          break;
        case "next":
          if (canNext) void serve();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [promo.active, doHint, doSkip, doGiveUp, canNext, serve]);

  /* ---- woodpecker ---- */
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
        setCycle({ set, cycleId, queue, idx: 0, solved: 0, totalMs: 0 });
        const first = queue[0];
        if (first) install({ puzzle: first, matchedThemes: [], allowed: 0, missed: 0 });
        setStatus(null);
      } catch (e) {
        setStatus(String(e));
      }
    },
    [install],
  );

  const doImport = useCallback(async () => {
    setImporting(true);
    setStatus("Importing puzzles — the full dump takes a few minutes…");
    try {
      const r = await importPuzzles(importPath, 50);
      setStatus(
        `Imported ${r.imported} puzzles (${r.duplicatesSkipped} duplicates, ${r.malformed} malformed) in ${formatClock(r.elapsedMs)}.`,
      );
      loadState();
    } catch (e) {
      setStatus(`Import failed: ${e}`);
    } finally {
      setImporting(false);
    }
  }, [importPath, loadState]);

  /* ---- derived display ---- */
  const elapsed = solvingNow ? Date.now() - startedAtRef.current : 0;
  const sessionDelta =
    st && sessionStartRatingRef.current !== null ? st.rating - sessionStartRatingRef.current : 0;
  const sideToMove = model
    ? `${model.solverColor === "white" ? "WHITE" : "BLACK"} TO MOVE`
    : "NO PUZZLE ON THE BOARD";
  const latestSet = sets[sets.length - 1] ?? null;
  const why = whyText(voice, mode, served, {
    theme: theme || undefined,
    cycleNo: cycle ? cycle.set.cycles + 1 : latestSet ? latestSet.cycles : undefined,
    setName: cycle?.set.name ?? latestSet?.name,
  });
  const hintShapes = hintSquare ? [{ orig: hintSquare, brush: "orange" }] : undefined;

  return (
    <div className="tx2">
      <ScreenHeader
        title="Tactics"
        subtitle={
          st
            ? `${st.puzzles.toLocaleString()} puzzles · rating ${st.rating.toFixed(0)} over ${st.attempts} attempts`
            : "Five drill modes · the weakness queue is seeded from your motif matrix"
        }
      />
      <div className="tx2-body">
        {/* ---- mode column ---- */}
        <div className="tx2-modes">
          <div className="tx2-modes-label">MODE</div>
          {seededMotif && (
            <div className="tx2-seed" title="Seeded by Train this weakness">
              <span className="tx2-seed-label">SEEDED</span>
              <span className="tx2-seed-motif">{seededMotif}</span>
              <button className="tx2-seed-clear" onClick={() => setSeededMotif(null)} title="Clear the seed">
                ×
              </button>
            </div>
          )}
          <div className="tx2-mode-list">
            {MODE_DEFS.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`tx2-mode${mode === m.id ? " cur" : ""}`}
                disabled={phase === "solving"}
                onClick={() => setMode(m.id)}
              >
                <span className="tx2-mode-row">
                  <span className="tx2-mode-name">{m.name}</span>
                  <span className="tx2-mode-badge">
                    {modeBadge(m.id, st, profile, sets.length)}
                  </span>
                </span>
                <span className="tx2-mode-note">{m.note}</span>
              </button>
            ))}
          </div>
          {mode === "motif" && (
            <label className="tx2-theme-pick">
              theme
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

          <div className="tx2-wood">
            <div className="tx2-wood-title">
              WOODPECKER{latestSet ? ` · ${latestSet.name.toUpperCase()}` : ""}
            </div>
            {latestSet ? (
              <>
                {setStats.length === 0 ? (
                  <div className="tx2-wood-empty">No cycles finished yet.</div>
                ) : (
                  <div className="tx2-wood-bars">
                    {setStats.slice(-3).map((c) => (
                      <div key={c.cycleId} className="tx2-wood-row">
                        <span className="tx2-wood-label">cycle {c.cycleNo}</span>
                        <span className="tx2-wood-bar">
                          <BaselineBar fraction={c.accuracyPct / 100} tone="good" />
                        </span>
                        <span className="tx2-wood-value">
                          {formatClock(c.totalTimeMs)} · {c.accuracyPct.toFixed(0)}%
                        </span>
                      </div>
                    ))}
                  </div>
                )}
                <button
                  className="btn-secondary tx2-wood-start"
                  disabled={cycle !== null || (st?.puzzles ?? 0) === 0}
                  onClick={() => void doStartCycle(latestSet)}
                >
                  {cycle ? `Cycle running · ${Math.min(cycle.idx + 1, cycle.queue.length)}/${cycle.queue.length}` : "Start next cycle"}
                </button>
              </>
            ) : (
              <>
                <div className="tx2-wood-empty">
                  No sets yet — a set is a fixed batch you re-solve, faster each cycle.
                </div>
                <div className="tx2-wood-create">
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
                  />
                  <button
                    className="btn-secondary"
                    onClick={() => void doCreateSet()}
                    disabled={!st || st.puzzles === 0}
                  >
                    Create
                  </button>
                </div>
              </>
            )}
          </div>
        </div>

        {/* ---- board column ---- */}
        <div className="tx2-board-col">
          {st && st.puzzles === 0 ? (
            <div className="tx2-import">
              <p className="tx2-import-prose">
                No puzzles imported yet. The Lichess puzzle database is CC0 — download
                lichess_db_puzzle.csv from database.lichess.org and import it here.
              </p>
              <div className="tx2-import-row">
                <input
                  type="text"
                  value={importPath}
                  onChange={(e) => setImportPath(e.target.value)}
                  placeholder={DEFAULT_CSV}
                />
                <button className="btn-primary" onClick={() => void doImport()} disabled={importing}>
                  {importing ? "Importing…" : "Import CSV"}
                </button>
              </div>
              {status && <div className="tx2-status">{status}</div>}
            </div>
          ) : (
            <>
              <div className="tx2-meta">
                <span className="tx2-meta-side">{sideToMove}</span>
                <span className="tx2-meta-spacer" />
                {streak > 0 && <span className="tx2-meta-streak">STREAK {streak}</span>}
                {st && (
                  <span className="tx2-meta-rating">
                    RATING {st.rating.toFixed(0)}{" "}
                    {sessionDelta !== 0 && (
                      <span className={sessionDelta > 0 ? "up" : "down"}>
                        {sessionDelta > 0 ? "+" : ""}
                        {sessionDelta.toFixed(0)}
                      </span>
                    )}
                  </span>
                )}
                {isTimedMode(mode) && solvingNow && (
                  <span className="tx2-clock">{formatClock(elapsed)}</span>
                )}
              </div>

              <div className="tx2-board">
                <Board
                  fen={fen ?? "8/8/8/8/8/8/8/8 w - - 0 1"}
                  lastMove={lastMove}
                  movable={movable}
                  orientation={model?.solverColor ?? "white"}
                  treatment={treatment}
                  size={560}
                  shapes={hintShapes}
                />
                {promo.element}
              </div>

              <div className="tx2-controls">
                {phase === "solving" ? (
                  <>
                    <button className="btn-secondary" onClick={doHint} disabled={!solvingNow}>
                      Hint <span className="tx2-key">H</span>
                    </button>
                    <button className="btn-secondary" onClick={doSkip} disabled={!solvingNow}>
                      Skip <span className="tx2-key">S</span>
                    </button>
                    <button className="btn-primary" onClick={doGiveUp} disabled={!solvingNow}>
                      Give up <span className="tx2-key">G</span>
                    </button>
                    <span className="tx2-kbd-hint">
                      Play the move on the board — H hint, S skip, G give up.
                    </span>
                  </>
                ) : (
                  <>
                    <button className="btn-primary" onClick={() => void serve()} disabled={!canNext || !st}>
                      {served ? "Next puzzle" : "Start solving"} <span className="tx2-key">⏎</span>
                    </button>
                    {finished && model && (
                      <span className="tx2-replay">
                        <button
                          className="btn-secondary"
                          onClick={() => setLineIdx((i) => Math.max(0, i - 1))}
                          disabled={lineIdx === 0}
                        >
                          ◀
                        </button>
                        <button
                          className="btn-secondary"
                          onClick={() => setLineIdx((i) => Math.min(model.fens.length - 1, i + 1))}
                          disabled={lineIdx >= model.fens.length - 1}
                        >
                          ▶
                        </button>
                        <span className="tx2-kbd-hint">replay the solution · ⏎ next</span>
                      </span>
                    )}
                  </>
                )}
              </div>
              {outcomeText && (
                <div className={`tx2-outcome ${phase}`}>
                  {phase === "solved" ? "Solved ✓ " : phase === "failed" ? "Failed ✗ " : ""}
                  {outcomeText}
                </div>
              )}
              {status && <div className="tx2-status">{status}</div>}
            </>
          )}
        </div>

        {/* ---- reasoning aside ---- */}
        <aside className="tx2-aside">
          <div className="tx2-aside-head">
            <span className="tx2-aside-title">WHY THIS PUZZLE</span>
            <span className="tx2-aside-spacer" />
            <div className="seg" role="tablist" aria-label="Narration voice">
              <button className={voice === "coach" ? "cur" : ""} onClick={() => onVoice("coach")}>
                Coach
              </button>
              <button className={voice === "neutral" ? "cur" : ""} onClick={() => onVoice("neutral")}>
                Neutral
              </button>
            </div>
          </div>
          <div className="tx2-aside-body">
            <p className="tx2-why-headline">{why.headline}</p>
            <p className="tx2-why-body">{why.body}</p>
            <div className="tx2-facts">
              <div className="tx2-fact">
                <span className="tx2-fact-label">MOTIF</span>
                <span className="tx2-fact-value">{motifFact(mode, served, finished)}</span>
              </div>
              <div className="tx2-fact">
                <span className="tx2-fact-label">SOURCE</span>
                <span className="tx2-fact-value">{sourceFact(mode, served)}</span>
              </div>
              <div className="tx2-fact">
                <span className="tx2-fact-label">RATING</span>
                <span className="tx2-fact-value mono">
                  {served ? served.puzzle.rating : "—"}
                </span>
              </div>
            </div>
            <div className="tx2-aside-footnote">
              {mode === "weakness"
                ? "Queue order comes from your motif matrix, not a generic ladder. Solve or fail, the result feeds straight back into your rating."
                : "Solve or fail, the result feeds straight back into your rating."}
            </div>
          </div>
          <div className="tx2-aside-footer">
            {mode === "woodpecker" && cycle ? (
              <>
                <span className="tx2-fact-label">QUEUE</span>
                <span className="tx2-queue-bar">
                  <BaselineBar
                    fraction={cycle.queue.length === 0 ? 0 : cycle.idx / cycle.queue.length}
                    tone="good"
                  />
                </span>
                <span className="tx2-queue-count">
                  {Math.min(cycle.idx + 1, cycle.queue.length)} / {cycle.queue.length}
                </span>
              </>
            ) : (
              <>
                <span className="tx2-fact-label">SESSION</span>
                <span className="tx2-queue-count">
                  {session.solved} / {session.attempts} solved
                </span>
              </>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

function solutionText(model: PuzzleModel): string {
  return model.sans.slice(1).join(" ");
}
