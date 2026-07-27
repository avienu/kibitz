/**
 * Openings SRS (design/handoff-2 §Openings SRS): 292px repertoire column |
 * board column | 340px session aside. The screen owns its header and its
 * board (size 560, walnut/instrument via the app treatment).
 *
 * Honesty rules: due counts, previews and lapse counts are real backend
 * data; grade buttons show the REAL next interval from the FSRS scheduler
 * (DueCard.previews — equal to what grading will set); the lapse paragraph
 * names the current card's actual branch or is absent. Repertoire lines
 * show the stored repertoire name (cards carry no ECO, so no opening-name
 * resolution is derivable). Per-line totals beyond the due queue are not
 * exposed by the backend, so only due counts render per line.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Board, { type BoardMovable } from "./Board";
import ScreenHeader from "./shell/ScreenHeader";
import { usePromotionPicker } from "./PromotionPicker";
import type { BoardShape } from "./lib/explainView";
import type { BoardTreatment } from "./lib/evidence";
import { isEditableTarget, type EditableTargetLike } from "./lib/gameView";
import type { PromoRole } from "./lib/promotion";
import {
  trainGrade,
  trainQueue,
  trainSummary,
  type DueCard,
  type TrainColor,
  type TrainGrade,
  type TrainSummary,
} from "./lib/db";
import {
  emptySummary,
  expectedArrow,
  fenAfterSan,
  formatInterval,
  sanForBoardMove,
  sanMatches,
  srsKeyAction,
  tallyAnswer,
  trainDests,
  type SessionSummary,
} from "./lib/train";

const GRADES: readonly { grade: TrainGrade; label: string; key: string; tone: string }[] = [
  { grade: "again", label: "Again", key: "1", tone: "bad" },
  { grade: "hard", label: "Hard", key: "2", tone: "dim" },
  { grade: "good", label: "Good", key: "3", tone: "good" },
  { grade: "easy", label: "Easy", key: "4", tone: "info" },
];

/**
 * The grade row (exported for tests): Again 1 / Hard 2 / Good 3 / Easy 4,
 * coloured bad/dim/good/info, each showing its REAL next interval from the
 * card's scheduler previews, formatted by lib/train.ts.
 */
export function GradeRow({
  card,
  onGrade,
  disabled,
}: {
  card: DueCard;
  onGrade: (g: TrainGrade) => void;
  disabled?: boolean;
}) {
  return (
    <div className="grade-row">
      {GRADES.map((g) => (
        <button
          key={g.grade}
          className={`grade-btn ${g.tone}`}
          disabled={disabled}
          onClick={() => onGrade(g.grade)}
        >
          <span className="grade-label">{g.label}</span>
          <span className="grade-key">{g.key}</span>
          <span className="grade-next">{formatInterval(card.previews[g.grade])}</span>
        </button>
      ))}
    </div>
  );
}

interface Session {
  cards: DueCard[];
  idx: number;
  tally: SessionSummary;
  /** prompt = waiting for an answer; correct/wrong = revealed, grade row live. */
  phase: "prompt" | "correct" | "wrong" | "done";
}

interface TrainViewProps {
  /** Reports fresh due counts (rail badge + status-strip nudge). */
  onSummary: (s: TrainSummary | null) => void;
  treatment: BoardTreatment;
}

/** Openings SRS screen — repertoire column, session board, session aside. */
export default function TrainView({ onSummary, treatment }: TrainViewProps) {
  const [color, setColor] = useState<TrainColor>("white");
  const [summary, setSummary] = useState<TrainSummary | null>(null);
  const [queue, setQueue] = useState<DueCard[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [typed, setTyped] = useState("");
  const [typedNote, setTypedNote] = useState<string | null>(null);
  const [showImport, setShowImport] = useState(false);
  const grading = useRef(false);

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

  const card = session && session.phase !== "done" ? session.cards[session.idx] : null;
  const revealed = session?.phase === "correct" || session?.phase === "wrong";

  /* ---- answering ---- */

  const answer = useCallback((correct: boolean) => {
    setTyped("");
    setTypedNote(null);
    setSession((s) =>
      s ? { ...s, phase: correct ? "correct" : "wrong", tally: tallyAnswer(s.tally, correct) } : s,
    );
  }, []);

  const handleBoardMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!card || revealed) return;
      const san = sanForBoardMove(card.fen, orig, dest, promoRole);
      if (!san) return;
      answer(sanMatches(card.expectedSan, san));
    },
    [card, revealed, answer],
  );

  const submitTyped = useCallback(() => {
    if (!card || revealed) return;
    const san = typed.trim();
    if (!san) return;
    if (sanMatches(card.expectedSan, san)) {
      answer(true);
      return;
    }
    // Any legal-but-different move is a wrong answer; garbage is a typo.
    if (fenAfterSan(card.fen, san)) answer(false);
    else setTypedNote(`"${san}" is not a legal move here.`);
  }, [card, revealed, typed, answer]);

  /* ---- grading (the four-button row; keyboard 1–4) ---- */

  const advance = useCallback(() => {
    setSession((s) => {
      if (!s) return s;
      const idx = s.idx + 1;
      return idx >= s.cards.length ? { ...s, idx, phase: "done" } : { ...s, idx, phase: "prompt" };
    });
  }, []);

  const grade = useCallback(
    (g: TrainGrade) => {
      if (!card || grading.current) return;
      grading.current = true;
      trainGrade(card.cardId, g)
        .then(() => void refresh())
        .catch((e) => setError(String(e)))
        .finally(() => {
          grading.current = false;
        });
      advance();
    },
    [card, advance, refresh],
  );

  const startSession = useCallback(async () => {
    try {
      const cards = await trainQueue(color, 100);
      if (cards.length === 0) return;
      setSession({ cards, idx: 0, tally: emptySummary(), phase: "prompt" });
      setTyped("");
      setTypedNote(null);
    } catch (e) {
      setError(String(e));
    }
  }, [color]);

  const endSession = useCallback(() => {
    setSession(null);
    void refresh();
  }, [refresh]);

  const pickColor = useCallback((c: TrainColor) => {
    setColor(c);
    setSession(null);
  }, []);

  /* ---- keyboard: 1–4 grade after reveal, ⏎ submits (never in inputs) ---- */
  useEffect(() => {
    if (!card) return;
    const onKey = (e: KeyboardEvent) => {
      const act = srsKeyAction(e.key, {
        editable: isEditableTarget(e.target as EditableTargetLike | null),
        revealed,
        modifier: e.metaKey || e.ctrlKey || e.altKey,
      });
      if (!act) return;
      e.preventDefault();
      if (act === "submit") submitTyped();
      else if (act === "grade-again") grade("again");
      else if (act === "grade-hard") grade("hard");
      else if (act === "grade-good") grade("good");
      else if (act === "grade-easy") grade("easy");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [card, revealed, submitTyped, grade]);

  /* ---- board state per phase ---- */
  const promoRef = useRef(handleBoardMove);
  promoRef.current = handleBoardMove;
  const promo = usePromotionPicker((orig, dest, role) => promoRef.current(orig, dest, role));

  const board = useMemo((): {
    fen: string;
    movable?: BoardMovable;
    lastMove?: [string, string];
    shapes?: BoardShape[];
  } | null => {
    if (!card) return null;
    if (session?.phase === "prompt") {
      return {
        fen: card.fen,
        movable: {
          color,
          dests: trainDests(card.fen),
          onMove: (orig, dest) => {
            if (!promo.guard(card.fen, orig, dest)) promoRef.current(orig, dest);
          },
        },
      };
    }
    if (session?.phase === "correct") {
      const uci = card.expectedUci;
      return {
        fen: fenAfterSan(card.fen, card.expectedSan) ?? card.fen,
        lastMove:
          uci.length >= 4 ? [uci.slice(0, 2), uci.slice(2, 4)] : undefined,
      };
    }
    // wrong: stay on the position and point at the expected move.
    const arrow = expectedArrow(card.fen, card.expectedSan);
    return { fen: card.fen, shapes: arrow ? [{ ...arrow, brush: "green" }] : undefined };
  }, [card, session?.phase, color, promo]);

  /* ---- derived display data ---- */

  const counts = summary ? summary[color] : null;

  /** Lines = the due queue grouped by repertoire name (real due counts). */
  const lines = useMemo(() => {
    const m = new Map<string, number>();
    for (const c of queue) m.set(c.repertoireName, (m.get(c.repertoireName) ?? 0) + 1);
    return [...m.entries()].map(([name, due]) => ({ name, due }));
  }, [queue]);

  const done = session?.tally.reviewed ?? 0;
  const lapses = session?.tally.again ?? 0;
  const newCount = queue.filter((c) => c.isNew).length;

  const metaLeft = card
    ? `${card.repertoireName} · your move as ${color}`.toUpperCase()
    : null;

  return (
    <>
      <ScreenHeader
        title="Openings SRS"
        subtitle={
          counts
            ? `FSRS scheduling · ${counts.due} due today · ${counts.total} positions as ${color}`
            : "FSRS scheduling · open a database to train"
        }
        actions={
          <button className="btn-secondary" onClick={() => setShowImport(true)}>
            Import repertoire
          </button>
        }
      />
      <div className="screen-cols">
        {/* ---- repertoire column (292px) ---- */}
        <div className="srs-col">
          <div className="srs-colhead">
            <span className="col-label">REPERTOIRE</span>
            <span className="seg" role="group" aria-label="Repertoire colour">
              {(["white", "black"] as const).map((c) => (
                <button key={c} className={color === c ? "cur" : ""} onClick={() => pickColor(c)}>
                  as {c === "white" ? "White" : "Black"}
                </button>
              ))}
            </span>
          </div>
          {error && <div className="error">{error}</div>}
          <div className="srs-lines">
            {lines.map((l) => (
              <div key={l.name} className="srs-line">
                <span className="srs-line-name">{l.name}</span>
                <span className={`srs-line-due${l.due > 0 ? " hot" : ""}`}>{l.due} due</span>
              </div>
            ))}
            {lines.length === 0 && (
              <p className="srs-empty">
                No cards due as {color}.{" "}
                {counts && counts.total > 0
                  ? `All ${counts.total} positions are scheduled ahead.`
                  : "Add lines from a loaded game (“→ repertoire” under the Moves panel) or import a study."}
              </p>
            )}
          </div>
          <button className="srs-import" onClick={() => setShowImport(true)}>
            Import PGN or Lichess study
          </button>
          <p className="srs-foot">
            Scheduling is FSRS-4.5. Each grade button shows the real next interval the scheduler
            will set for that answer.
          </p>
        </div>

        {/* ---- board column ---- */}
        <div className="srs-board-col">
          {card && board ? (
            <>
              <div className="srs-meta">
                <span className="srs-meta-line">{metaLeft}</span>
                <span className="flex-spacer" />
                {card.lapses > 0 && <span className="lapse-pill">LAPSE ×{card.lapses}</span>}
                <span className="srs-meta-counts">
                  {counts ? `${counts.due} DUE · ${done} DONE` : `${done} DONE`}
                </span>
              </div>
              <div className="srs-board">
                <Board
                  fen={board.fen}
                  orientation={color}
                  movable={board.movable}
                  lastMove={board.lastMove}
                  shapes={board.shapes}
                  treatment={treatment}
                  size={560}
                />
                {promo.element}
              </div>
              {!revealed ? (
                <div className="srs-answer">
                  <input
                    className="srs-san"
                    type="text"
                    value={typed}
                    placeholder="type your move…"
                    spellCheck={false}
                    onChange={(e) => {
                      setTyped(e.target.value);
                      setTypedNote(null);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        submitTyped();
                      }
                    }}
                  />
                  <span className="srs-answer-hint">or play it on the board</span>
                  {typedNote && <span className="srs-typed-note">{typedNote}</span>}
                </div>
              ) : (
                <div className="srs-reveal">
                  {session?.phase === "correct" ? (
                    <span className="srs-reveal-ok">
                      Correct — <b>{card.expectedSan}</b>. How well did you know it?
                    </span>
                  ) : (
                    <span className="srs-reveal-bad">
                      The repertoire move is <b>{card.expectedSan}</b>.
                    </span>
                  )}
                </div>
              )}
              <GradeRow card={card} onGrade={grade} disabled={!revealed} />
            </>
          ) : session?.phase === "done" ? (
            <div className="srs-start">
              <div className="srs-start-title">Session complete</div>
              <p className="srs-start-prose">
                {session.tally.reviewed} reviewed — {session.tally.correct} correct,{" "}
                {session.tally.again} to relearn.
              </p>
              <button className="btn-primary" onClick={endSession}>
                Back to the queue
              </button>
            </div>
          ) : (
            <div className="srs-start">
              <div className="srs-start-title">
                {counts ? `${counts.due} due as ${color}` : "No database open"}
              </div>
              {counts && counts.total === 0 && (
                <p className="srs-start-prose">
                  No cards yet. Add lines from a loaded game (&ldquo;→ repertoire&rdquo; under the
                  Moves panel) or import a PGN study.
                </p>
              )}
              <button
                className="btn-primary"
                onClick={() => void startSession()}
                disabled={!counts || counts.due === 0}
              >
                Start review
              </button>
            </div>
          )}
        </div>

        {/* ---- session aside (340px) ---- */}
        <aside className="srs-aside">
          <div className="aside-label">THIS SESSION</div>
          <div className="srs-tiles">
            <div className="stat-tile">
              <div className="stat-tile-caption">DUE</div>
              <div className="stat-tile-row">
                <span className="stat-tile-value">{counts?.due ?? "—"}</span>
              </div>
            </div>
            <div className="stat-tile">
              <div className="stat-tile-caption">DONE</div>
              <div className="stat-tile-row">
                <span className="stat-tile-value">{done}</span>
              </div>
            </div>
            <div className="stat-tile">
              <div className="stat-tile-caption">LAPSES</div>
              <div className="stat-tile-row">
                <span className="stat-tile-value">{lapses}</span>
              </div>
            </div>
            <div className="stat-tile">
              <div className="stat-tile-caption">NEW</div>
              <div className="stat-tile-row">
                <span className="stat-tile-value">{newCount}</span>
              </div>
            </div>
          </div>
          {card && card.lapses > 0 && (
            <p className="srs-prose">
              You keep lapsing on {card.linePrefix || "the start position"} —{" "}
              <b>{card.expectedSan}</b> in {card.repertoireName} has lapsed {card.lapses}{" "}
              {card.lapses === 1 ? "time" : "times"}. It stays in the queue until you answer it
              cleanly.
            </p>
          )}
        </aside>
      </div>

      {showImport && (
        <div className="modal-overlay" onClick={() => setShowImport(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-title">Import a repertoire</div>
            <p className="modal-prose">
              Bulk import is a command-line feature for now — every mainline move of your colour
              in the PGN (a Lichess study export works) becomes a training card. Re-import is
              idempotent: positions that already have a card are left untouched.
            </p>
            <pre className="modal-cli">
              {"silman-cli --db <path.sqlite> import-repertoire study.pgn " + color + ' --name "main"'}
            </pre>
            <p className="modal-prose">
              From inside the app, load a game and use &ldquo;→ repertoire&rdquo; under the Moves
              panel to add one line at a time.
            </p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => setShowImport(false)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
