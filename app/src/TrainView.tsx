import { useCallback, useEffect, useMemo, useState } from "react";
import type { BoardMovable } from "./Board";
import type { BoardShape } from "./lib/explainView";
import type { PromoRole } from "./lib/promotion";
import {
  trainGrade,
  trainQueue,
  trainSummary,
  type DueCard,
  type TrainColor,
  type TrainSummary,
} from "./lib/db";
import {
  emptySummary,
  expectedArrow,
  fenAfterSan,
  formatInterval,
  sanForBoardMove,
  sanMatches,
  tallyAnswer,
  trainDests,
  type SessionSummary,
} from "./lib/train";

/** What the Train tab wants shown on the main board during a session.
 * `onMove` accepts the promotion-picker role (run-6 item 3); the board
 * host guards drags with the picker before invoking it. */
export interface TrainMovable extends BoardMovable {
  onMove: (orig: string, dest: string, promoRole?: PromoRole) => void;
}

export interface TrainBoardState {
  fen: string;
  orientation: TrainColor;
  movable?: TrainMovable;
  shapes?: BoardShape[];
}

interface Session {
  cards: DueCard[];
  idx: number;
  tally: SessionSummary;
  /** prompt = waiting for a board move; correct/wrong = feedback shown. */
  phase: "prompt" | "correct" | "wrong" | "done";
  /** Interval feedback for the last graded answer. */
  lastInterval?: number;
}

interface TrainViewProps {
  /** Reports fresh due counts (Train tab badge). */
  onSummary: (s: TrainSummary | null) => void;
  /** Overrides the main board while a session runs (null = release). */
  onBoard: (b: TrainBoardState | null) => void;
}

/** Right-hand panel: Repertoire Trainer — due queue and review sessions. */
export default function TrainView({ onSummary, onBoard }: TrainViewProps) {
  const [color, setColor] = useState<TrainColor>("white");
  const [summary, setSummary] = useState<TrainSummary | null>(null);
  const [queue, setQueue] = useState<DueCard[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<Session | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await trainSummary();
      setSummary(s);
      onSummary(s);
      setQueue(await trainQueue(color, 100));
      setError(null);
    } catch (e) {
      setSummary(null);
      setQueue([]);
      setError(String(e));
    }
  }, [color, onSummary]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Release the main board when leaving the tab mid-session.
  useEffect(() => () => onBoard(null), [onBoard]);

  const card = session && session.phase !== "done" ? session.cards[session.idx] : null;

  /** Advance to the next card, or finish the session. */
  const advance = useCallback(() => {
    setSession((s) => {
      if (!s) return s;
      const idx = s.idx + 1;
      return idx >= s.cards.length
        ? { ...s, idx, phase: "done" }
        : { ...s, idx, phase: "prompt", lastInterval: undefined };
    });
  }, []);

  const handleMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!card) return;
      const san = sanForBoardMove(card.fen, orig, dest, promoRole);
      if (!san) return;
      if (sanMatches(card.expectedSan, san)) {
        setSession((s) => (s ? { ...s, phase: "correct" } : s));
      } else {
        // Wrong: show the expected move; the card lapses immediately.
        setSession((s) =>
          s ? { ...s, phase: "wrong", tally: tallyAnswer(s.tally, false) } : s,
        );
        trainGrade(card.cardId, "again")
          .then((g) => {
            setSession((s) => (s ? { ...s, lastInterval: g.intervalDays } : s));
            void refresh();
          })
          .catch((e) => setError(String(e)));
      }
    },
    [card, refresh],
  );

  /** Grade a correctly answered card and move on. */
  const gradeCorrect = useCallback(
    (grade: "good" | "easy") => {
      if (!card) return;
      setSession((s) => (s ? { ...s, tally: tallyAnswer(s.tally, true) } : s));
      trainGrade(card.cardId, grade)
        .then(() => void refresh())
        .catch((e) => setError(String(e)));
      advance();
    },
    [card, advance, refresh],
  );

  const startSession = useCallback(async () => {
    try {
      const cards = await trainQueue(color, 100);
      if (cards.length === 0) return;
      setSession({ cards, idx: 0, tally: emptySummary(), phase: "prompt" });
    } catch (e) {
      setError(String(e));
    }
  }, [color]);

  const endSession = useCallback(() => {
    setSession(null);
    onBoard(null);
    void refresh();
  }, [onBoard, refresh]);

  // Publish the board override for the current card/phase.
  const boardState = useMemo((): TrainBoardState | null => {
    if (!card) return null;
    if (session?.phase === "prompt") {
      return {
        fen: card.fen,
        orientation: color,
        movable: { color, dests: trainDests(card.fen), onMove: handleMove },
      };
    }
    if (session?.phase === "correct") {
      return {
        fen: fenAfterSan(card.fen, card.expectedSan) ?? card.fen,
        orientation: color,
      };
    }
    // wrong: stay on the position and point at the expected move.
    const arrow = expectedArrow(card.fen, card.expectedSan);
    return {
      fen: card.fen,
      orientation: color,
      shapes: arrow ? [{ ...arrow, brush: "green" }] : undefined,
    };
  }, [card, session?.phase, color, handleMove]);

  useEffect(() => {
    onBoard(boardState);
  }, [boardState, onBoard]);

  const counts = summary ? summary[color] : null;

  if (session) {
    const { tally, phase } = session;
    return (
      <div className="train">
        <h3>Repertoire Trainer — {color}</h3>
        {phase === "done" ? (
          <div className="train-summary">
            <p>Session complete.</p>
            <p>
              {tally.reviewed} reviewed — {tally.correct} correct, {tally.again} to relearn.
            </p>
            <button onClick={endSession}>Back to queue</button>
          </div>
        ) : (
          card && (
            <div className="train-card">
              <div className="train-progress">
                card {session.idx + 1}/{session.cards.length}
                {card.isNew && <span className="train-new">new</span>}
                <span className="train-rep">{card.repertoireName}</span>
              </div>
              <div className="train-prompt">
                {card.linePrefix ? card.linePrefix : "Start position"}
                <b> — your move</b>
              </div>
              {phase === "prompt" && <div className="train-hint">Play your repertoire move on the board.</div>}
              {phase === "correct" && (
                <div className="train-feedback ok">
                  <span>
                    Correct: <b>{card.expectedSan}</b>. How well did you know it?
                  </span>
                  <button onClick={() => gradeCorrect("good")}>Good</button>
                  <button onClick={() => gradeCorrect("easy")}>Easy</button>
                </div>
              )}
              {phase === "wrong" && (
                <div className="train-feedback bad">
                  <span>
                    The repertoire move is <b>{card.expectedSan}</b>
                    {session.lastInterval !== undefined &&
                      ` — again in ${formatInterval(session.lastInterval)}`}
                    .
                  </span>
                  <button onClick={advance}>Continue</button>
                </div>
              )}
              <button className="train-abort" onClick={endSession}>
                End session
              </button>
            </div>
          )
        )}
        {error && <div className="error">{error}</div>}
      </div>
    );
  }

  return (
    <div className="train">
      <h3>Repertoire Trainer</h3>
      <div className="train-controls">
        {(["white", "black"] as const).map((c) => (
          <button key={c} className={color === c ? "cur" : ""} onClick={() => setColor(c)}>
            {c === "white" ? "White" : "Black"}
            {summary && summary[c].due > 0 && <span className="train-due">{summary[c].due}</span>}
          </button>
        ))}
        <button
          className="train-start"
          onClick={() => void startSession()}
          disabled={!counts || counts.due === 0}
        >
          Start review
        </button>
      </div>
      {error && <div className="error">{error}</div>}
      {counts && (
        <div className="train-counts">
          {counts.due} due of {counts.total} cards in the {color} repertoire.
        </div>
      )}
      {counts && counts.total === 0 && (
        <p className="train-empty">
          No cards yet. Add lines from a loaded game ("→ repertoire" under the board) or import a
          PGN study with <code>silman-cli import-repertoire</code>.
        </p>
      )}
      {queue.length > 0 && (
        <table className="train-queue">
          <thead>
            <tr>
              <th>line</th>
              <th>due</th>
              <th>reps</th>
            </tr>
          </thead>
          <tbody>
            {queue.slice(0, 20).map((c) => (
              <tr key={c.cardId}>
                <td className="tq-line">{c.linePrefix || "start"}</td>
                <td>{c.isNew ? "new" : c.due.slice(0, 10)}</td>
                <td>
                  {c.reps}
                  {c.lapses > 0 ? ` (${c.lapses} lapses)` : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
