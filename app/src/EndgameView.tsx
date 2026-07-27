/**
 * Endgames (design/handoff-2 §Endgames): 300px curriculum | board column |
 * 380px feedback aside. The user plays the goal side against the
 * tablebase/heuristic defender; every user move is graded ONLY against
 * tablebase truth (StepReport rows), never an engine score.
 *
 * Honesty rules: the header's verification label states TABLEBASE TRUTH
 * (with the real covered piece count) only when the defender actually
 * probes the tablebase, else names the heuristic defender; the curriculum
 * stores no key squares, so no key-square wedge is drawn; the instruction
 * is hidden behind "Show the idea".
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Board, { type BoardMovable } from "./Board";
import ScreenHeader from "./shell/ScreenHeader";
import BaselineBar from "./components/BaselineBar";
import { usePromotionPicker } from "./PromotionPicker";
import type { BoardTreatment } from "./lib/evidence";
import type { PromoRole } from "./lib/promotion";
import {
  destsFor,
  endgameGiveUp,
  endgameMove,
  endgameOverview,
  endgameStart,
  goalText,
  lastMoveOf,
  uciForDrag,
  type DrillInfo,
  type DrillProgress,
  type Outcome,
  type Overview,
  type StartedDrill,
  type VerdictRow,
} from "./lib/endgame";

type Phase = "playing" | "solved" | "failed";

interface PlayState {
  started: StartedDrill;
  fen: string;
  lastMove?: [string, string];
  phase: Phase;
  outcomeText: string;
  /** Accumulated feedback rows (StepReport rows arrive per step). */
  rows: VerdictRow[];
  showIdea: boolean;
}

const VERDICT_LABEL: Record<VerdictRow["verdict"], string> = {
  winning: "WINNING",
  slower: "SLOWER",
  throws: "THROWS",
  unverified: "UNVERIFIED",
  engine: "ENGINE",
};

/**
 * Feedback rows (exported for tests): `no | SAN | verdict | note`.
 * WINNING --good · SLOWER --accent (note states the DTZ cost) ·
 * THROWS --bad · ENGINE --faint · UNVERIFIED --faint italic.
 */
export function FeedbackRows({ rows }: { rows: VerdictRow[] }) {
  return (
    <div className="eg-rows">
      {rows.map((r) => (
        <div key={r.index} className="eg-row">
          <span className="eg-no">{r.index}.</span>
          <span className="eg-san">{r.san}</span>
          <span className={`eg-verdict v-${r.verdict}`}>{VERDICT_LABEL[r.verdict]}</span>
          <span className="eg-note">
            {r.note ||
              (r.verdict === "slower" && r.dtzCost != null ? `DTZ +${r.dtzCost} plies.` : "")}
          </span>
        </div>
      ))}
    </div>
  );
}

interface EndgameViewProps {
  treatment: BoardTreatment;
}

export default function EndgameView({ treatment }: EndgameViewProps) {
  const [ov, setOv] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tierId, setTierId] = useState<string | null>(null);
  const [play, setPlay] = useState<PlayState | null>(null);
  const busyRef = useRef(false); // one move in flight at a time
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadOverview = useCallback(() => {
    endgameOverview()
      .then((o) => {
        setOv(o);
        setError(null);
        setTierId((t) => t ?? o.tiers[0]?.id ?? null);
      })
      .catch((e) => setError(`Endgames unavailable (open a database first): ${e}`));
  }, []);
  useEffect(loadOverview, [loadOverview]);
  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    },
    [],
  );

  const finish = useCallback(
    (outcome: Outcome, progress: DrillProgress | null) => {
      setPlay((p) => {
        if (!p) return p;
        let note = "";
        if (progress) {
          note = progress.mastered
            ? " Drill mastered."
            : progress.cleanStreak > 0
              ? ` Clean streak ${progress.cleanStreak}/${ov?.masteryStreak ?? 2}.`
              : "";
        }
        return {
          ...p,
          phase: outcome.solved ? "solved" : "failed",
          outcomeText: `${outcome.solved ? "Solved — " : "Failed — "}${outcome.detail}${note}`,
        };
      });
      loadOverview();
    },
    [ov, loadOverview],
  );

  const start = useCallback(async (drillId: string) => {
    try {
      if (timerRef.current) clearTimeout(timerRef.current);
      const started = await endgameStart(drillId);
      setPlay({
        started,
        fen: started.fen,
        phase: "playing",
        outcomeText: "",
        rows: [],
        showIdea: false,
      });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Promotion picker: defers pawn-to-last-rank drags to the overlay.
  const moveHandlerRef = useRef<(orig: string, dest: string, promoRole?: PromoRole) => void>(
    () => {},
  );
  const promo = usePromotionPicker((orig, dest, role) => moveHandlerRef.current(orig, dest, role));

  const onBoardMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!play || play.phase !== "playing" || busyRef.current) return;
      if (!promoRole && promo.guard(play.fen, orig, dest)) return;
      const uci = uciForDrag(play.fen, orig, dest, promoRole);
      if (!uci) return;
      busyRef.current = true;
      endgameMove(uci)
        .then((r) => {
          // Rows accumulate client-side, per step (user row + engine row).
          setPlay((p) =>
            p
              ? { ...p, fen: r.fenAfterUser, lastMove: lastMoveOf(uci), rows: [...p.rows, ...r.rows] }
              : p,
          );
          const opp = r.opponent;
          if (opp && r.fenAfterOpponent) {
            const oppFen = r.fenAfterOpponent;
            timerRef.current = setTimeout(() => {
              setPlay((p) => (p ? { ...p, fen: oppFen, lastMove: lastMoveOf(opp.uci) } : p));
              if (r.outcome) finish(r.outcome, r.progress);
            }, 350);
          } else if (r.outcome) {
            finish(r.outcome, r.progress);
          }
        })
        .catch((e) => setError(String(e)))
        .finally(() => {
          busyRef.current = false;
        });
    },
    [play, finish, promo],
  );
  moveHandlerRef.current = onBoardMove;

  const giveUp = useCallback(async () => {
    try {
      const r = await endgameGiveUp();
      finish({ solved: false, detail: "gave up." }, r.progress);
    } catch (e) {
      setError(String(e));
    }
  }, [finish]);

  const restart = useCallback(() => {
    if (!play) return;
    if (play.phase === "playing") {
      // Restarting mid-attempt concedes it first (recorded honestly).
      void endgameGiveUp()
        .catch(() => {})
        .then(() => void start(play.started.drillId));
    } else {
      void start(play.started.drillId);
    }
  }, [play, start]);

  const movable = useMemo((): BoardMovable | undefined => {
    if (!play || play.phase !== "playing") return undefined;
    const dests = destsFor(play.fen, play.started.userSide);
    if (dests.size === 0) return undefined; // opponent's turn (reply pending)
    return { color: play.started.userSide, dests, onMove: onBoardMove };
  }, [play, onBoardMove]);

  const drillsOfTier = useCallback(
    (tier: string): DrillInfo[] => (ov ? ov.drills.filter((d) => d.tier === tier) : []),
    [ov],
  );

  const masteredTotal = ov ? ov.drills.filter((d) => d.mastered).length : 0;

  /** Verification label — honest: TABLEBASE TRUTH only when the defender
   * actually probes the tablebase for this drill. */
  const verification = play
    ? play.started.opponentTablebase && ov?.tablebase.available
      ? {
          text: `TABLEBASE TRUTH${ov.tablebase.largest != null ? ` · ${ov.tablebase.largest} PIECES` : ""}`,
          good: true,
        }
      : { text: "HEURISTIC DEFENDER · TERMINAL GRADING", good: false }
    : null;

  return (
    <>
      <ScreenHeader
        title="Endgames"
        subtitle={
          ov
            ? `Rating-tiered curriculum · ${ov.drills.length} drills · ${masteredTotal} mastered · ` +
              (ov.tablebase.available ? "Syzygy tablebase truth" : "no tablebase found")
            : "Rating-tiered curriculum"
        }
      />
      <div className="screen-cols">
        {/* ---- curriculum column (300px) ---- */}
        <div className="eg-col">
          <div className="col-label">CURRICULUM</div>
          {error && <div className="error">{error}</div>}
          {ov?.tiers.map((t) => {
            const drills = drillsOfTier(t.id);
            const mastered = drills.filter((d) => d.mastered).length;
            const complete = drills.length > 0 && mastered === drills.length;
            const active = t.id === tierId;
            return (
              <div key={t.id}>
                <button
                  className={`eg-tier${active ? " active" : ""}${complete ? " complete" : ""}`}
                  onClick={() => setTierId(t.id)}
                  title={t.summary}
                >
                  <span className="eg-tier-head">
                    <span className="eg-tier-name">{t.name}</span>
                    <span className="eg-tier-count">
                      {mastered} / {drills.length}
                    </span>
                  </span>
                  <BaselineBar
                    fraction={drills.length > 0 ? mastered / drills.length : 0}
                    tone="good"
                  />
                </button>
                {active && (
                  <div className="eg-drills">
                    {drills.map((d) => (
                      <button
                        key={d.id}
                        className={`eg-drill${play?.started.drillId === d.id ? " cur" : ""}`}
                        onClick={() => void start(d.id)}
                        title={d.concept}
                      >
                        <span className="eg-drill-name">
                          {d.mastered ? "✓ " : ""}
                          {d.title}
                        </span>
                        <span className="eg-drill-meta">{d.material}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* ---- board column ---- */}
        <div className="eg-board-col">
          {play ? (
            <>
              <div className="eg-meta">
                <span className="eg-meta-title">{play.started.title.toUpperCase()}</span>
                <span className="flex-spacer" />
                {verification && (
                  <span className={`eg-tb${verification.good ? " good" : ""}`}>
                    {verification.text}
                  </span>
                )}
              </div>
              <div className="eg-board">
                <Board
                  fen={play.fen}
                  lastMove={play.lastMove}
                  movable={movable}
                  orientation={play.started.userSide}
                  treatment={treatment}
                  size={560}
                />
                {promo.element}
              </div>
              <div className="eg-objective">
                <span className="eg-objective-text">
                  {play.phase === "playing"
                    ? `Objective: ${goalText(play.started.goal, play.started.userSide).toLowerCase()}. ` +
                      (play.started.opponentTablebase
                        ? "The defender replies from the tablebase."
                        : "The defender is a deterministic heuristic.")
                    : play.outcomeText}
                </span>
                <span className="flex-spacer" />
                <button className="btn-secondary" onClick={restart}>
                  Restart
                </button>
                {play.phase === "playing" ? (
                  <>
                    <button
                      className="btn-secondary"
                      onClick={() => setPlay((p) => (p ? { ...p, showIdea: !p.showIdea } : p))}
                    >
                      {play.showIdea ? "Hide the idea" : "Show the idea"}
                    </button>
                    <button className="btn-secondary" onClick={() => void giveUp()}>
                      Give up
                    </button>
                  </>
                ) : null}
              </div>
              {play.showIdea && play.phase === "playing" && (
                <p className="eg-idea">{play.started.instruction}</p>
              )}
            </>
          ) : (
            <div className="srs-start">
              <div className="srs-start-title">
                {ov ? "Pick a drill from the curriculum" : "No database open"}
              </div>
              <p className="srs-start-prose">
                You play the side to move against the toughest defence available — tablebase
                replies where the piece count is covered, a deterministic heuristic otherwise. No
                engine anywhere in this flow.
              </p>
            </div>
          )}
        </div>

        {/* ---- feedback aside (380px) ---- */}
        <aside className="eg-aside">
          <div className="aside-label">DRILL FEEDBACK</div>
          {play && play.rows.length > 0 ? (
            <FeedbackRows rows={play.rows} />
          ) : (
            <p className="eg-note-empty">
              {play ? "Play a move — every move gets a graded row here." : "No drill running."}
            </p>
          )}
          <p className="eg-closing">
            Every move is graded against the tablebase, never an engine score:{" "}
            <b>still winning</b>, <b>throws the win</b>, or <b>slower but winning</b> with the DTZ
            cost. Moves outside tablebase coverage are marked <b>unverified</b> and graded only at
            terminal positions.
          </p>
        </aside>
      </div>
    </>
  );
}
