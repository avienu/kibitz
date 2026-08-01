/**
 * Endgame drill view-model (pure): the client-side state machine that keeps
 * the drill loop alive and visible — `userTurn → replying → userTurn → … →
 * solved | failed`. The IPC layer (lib/endgame.ts) stays dumb; EndgameView
 * only schedules the reply beat and renders what this module derives.
 * No DOM, no Tauri — the whole loop is unit-testable.
 */
import {
  goalText,
  lastMoveOf,
  type DrillProgress,
  type MoveResponse,
  type Outcome,
  type StartedDrill,
  type VerdictRow,
} from "./endgame";

/** How long the defender's reply is held back so the user sees two distinct
 * moves instead of one teleport. */
export const REPLY_BEAT_MS = 350;

export type EndgamePhase = "userTurn" | "replying" | "solved" | "failed";

/** Defender reply held back for its animation beat (phase "replying"). */
export interface PendingReply {
  fen: string;
  uci: string;
  /** Terminal that the reply itself produced (mate/draw), if any. */
  outcome: Outcome | null;
  progress: DrillProgress | null;
}

export interface EndgameModel {
  started: StartedDrill;
  /** Position currently ON the board (mid-beat this is after the user's
   * move, before the pending reply). */
  fen: string;
  lastMove?: [string, string];
  phase: EndgamePhase;
  /** Accumulated feedback rows (StepReport rows arrive per step). */
  rows: VerdictRow[];
  userMoves: number;
  outcome: Outcome | null;
  progress: DrillProgress | null;
  pendingReply: PendingReply | null;
}

export function beginDrill(started: StartedDrill): EndgameModel {
  return {
    started,
    fen: started.fen,
    lastMove: undefined,
    phase: "userTurn",
    rows: [],
    userMoves: 0,
    outcome: null,
    progress: null,
    pendingReply: null,
  };
}

/** Whether the board should accept user input. */
export function canMove(m: EndgameModel): boolean {
  return m.phase === "userTurn";
}

export function isTerminal(m: EndgameModel): boolean {
  return m.phase === "solved" || m.phase === "failed";
}

/**
 * Fold one `endgame_move` response in. When the drill continues, the
 * defender's reply is parked on `pendingReply` (phase "replying") for the
 * caller to commit after [`REPLY_BEAT_MS`]; a terminal without a reply
 * (mate delivered, result thrown, drawn terminal) lands immediately.
 */
export function applyMoveResponse(
  m: EndgameModel,
  userUci: string,
  r: MoveResponse,
): EndgameModel {
  const base: EndgameModel = {
    ...m,
    fen: r.fenAfterUser,
    lastMove: lastMoveOf(userUci),
    rows: [...m.rows, ...r.rows],
    userMoves: m.userMoves + 1,
  };
  if (r.opponent && r.fenAfterOpponent) {
    return {
      ...base,
      phase: "replying",
      pendingReply: {
        fen: r.fenAfterOpponent,
        uci: r.opponent.uci,
        outcome: r.outcome,
        progress: r.progress,
      },
    };
  }
  if (r.outcome) {
    return {
      ...base,
      phase: r.outcome.solved ? "solved" : "failed",
      outcome: r.outcome,
      progress: r.progress,
      pendingReply: null,
    };
  }
  // Defensive: the backend always sends a reply or an outcome.
  return { ...base, phase: "userTurn", pendingReply: null };
}

/** Land the parked defender reply: back to the user's turn, or terminal
 * when the reply itself ended the drill. No-op without a pending reply. */
export function commitReply(m: EndgameModel): EndgameModel {
  const p = m.pendingReply;
  if (!p) return m;
  const base: EndgameModel = {
    ...m,
    fen: p.fen,
    lastMove: lastMoveOf(p.uci),
    pendingReply: null,
  };
  if (p.outcome) {
    return {
      ...base,
      phase: p.outcome.solved ? "solved" : "failed",
      outcome: p.outcome,
      progress: p.progress,
    };
  }
  return { ...base, phase: "userTurn" };
}

/** Concede: terminal failed state with the recorded progress. */
export function applyGiveUp(
  m: EndgameModel,
  progress: DrillProgress,
): EndgameModel {
  if (isTerminal(m)) return m;
  return {
    ...m,
    phase: "failed",
    outcome: { solved: false, detail: "Gave up." },
    progress,
    pendingReply: null,
  };
}

/* ---- derived display ------------------------------------------------------ */

export interface StatusLine {
  tone: "play" | "wait" | "good" | "bad";
  text: string;
}

/** The unmissable one-liner over the board: whose turn, what the goal is,
 * or how the drill ended. */
export function statusLine(m: EndgameModel): StatusLine {
  switch (m.phase) {
    case "userTurn": {
      const goal = goalText(m.started.goal, m.started.userSide);
      return {
        tone: "play",
        text: `Your move — ${goal.charAt(0).toLowerCase()}${goal.slice(1)}.`,
      };
    }
    case "replying":
      return { tone: "wait", text: "Defender is thinking…" };
    case "solved":
      return {
        tone: "good",
        text: `Solved — ${m.outcome?.detail ?? ""}`.trim(),
      };
    case "failed":
      return {
        tone: "bad",
        text: `Failed — ${m.outcome?.detail ?? ""}`.trim(),
      };
  }
}

/** How far a drill has got, for the curriculum list. Mastery takes
 * `masteryStreak` consecutive clean solves, so "solved it" and "mastered
 * it" are different states — showing only the second means finishing a
 * drill changes nothing on screen, which reads as the app not having
 * noticed (2026-08-01 field report). */
export type DrillState = "mastered" | "solved" | "tried" | "new";

export interface DrillMark {
  state: DrillState;
  /** Leading glyph for the row ("" for an untouched drill). */
  mark: string;
  /** Tooltip/aria text — always the real counts, never a guess. */
  label: string;
}

export function drillMark(
  d: {
    attempts: number;
    solved: number;
    cleanStreak: number;
    mastered: boolean;
  },
  masteryStreak: number,
): DrillMark {
  if (d.mastered) {
    return {
      state: "mastered",
      mark: "✓",
      label: `Mastered · solved ${d.solved}×`,
    };
  }
  if (d.solved > 0) {
    return {
      state: "solved",
      mark: "◍",
      label:
        `Solved ${d.solved}× · clean streak ${d.cleanStreak}/${masteryStreak}` +
        ` — ${Math.max(1, masteryStreak - d.cleanStreak)} more in a row to master`,
    };
  }
  if (d.attempts > 0) {
    return {
      state: "tried",
      mark: "·",
      label: `Tried ${d.attempts}× · not solved yet`,
    };
  }
  return { state: "new", mark: "", label: "Not attempted" };
}

/** Mastery/streak note for the terminal panel ("" before any progress). */
export function progressNote(m: EndgameModel, masteryStreak: number): string {
  const p = m.progress;
  if (!p) return "";
  if (p.mastered) return "Drill mastered.";
  if (p.cleanStreak > 0)
    return `Clean streak ${p.cleanStreak}/${masteryStreak}.`;
  return "";
}

/** What went wrong, for the failure panel: the last THROWS verdict row
 * (the move that lost the goal), or null when no graded move did (e.g.
 * gave up, or an unverified stretch ended in a draw). */
export function failureReason(m: EndgameModel): string | null {
  if (m.phase !== "failed") return null;
  for (let i = m.rows.length - 1; i >= 0; i--) {
    const r = m.rows[i];
    if (r.verdict === "throws") {
      return r.note
        ? `${r.san} — ${r.note}`
        : `${r.san} threw the result away.`;
    }
  }
  return null;
}

/**
 * The Next-drill affordance: the first UNMASTERED drill after the current
 * one in curriculum order (wrapping), else simply the next drill, else
 * null when the current drill is the only one.
 */
export function nextDrillId(
  order: readonly { id: string; mastered: boolean }[],
  currentId: string,
): string | null {
  const at = order.findIndex((d) => d.id === currentId);
  if (at < 0 || order.length < 2) return null;
  for (let step = 1; step < order.length; step++) {
    const d = order[(at + step) % order.length];
    if (!d.mastered) return d.id;
  }
  return order[(at + 1) % order.length].id;
}
