/**
 * Hover-scrub line preview (2026-07-30 field request: "hard to imagine"
 * prospective repertoire lines from their SAN text).
 *
 * Renders a SAN line as per-move tokens, numbered exactly like the plain
 * numberedLine text ("1. e4 c5 2. Nf3"). Hovering token i reports the
 * position AFTER move i+1 (fen + from/to squares) through onPreview so
 * the caller can drive its board; mouse leaving the whole line reports
 * null. The replay (chessops via gameFromSans) happens ONCE in a memo —
 * the hover path is a synchronous array lookup, nothing async.
 *
 * A line that does not replay legally from its start position renders as
 * plain text and never calls onPreview.
 *
 * Keyboard: the line is focusable; ←/→ step the preview, Home/End jump,
 * Esc clears. Blur clears. Unmounting while this line owns the live
 * preview clears it too (lists rebuild under the cursor — mouseleave
 * never fires then).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { gameFromSans, lastMoveAt } from "../lib/game";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

export interface ScrubPreview {
  /** Position after `ply` moves of the line. */
  fen: string;
  /** From/to squares of move `ply` — the board's last-move highlight. */
  lastMove: [string, string] | null;
  /** 1-based move index into the line (the position is AFTER move ply). */
  ply: number;
  /** Move `ply` with its number, e.g. "3. Nf3" / "1... c5" — caption text. */
  label: string;
}

export interface ScrubToken {
  /** Display token, numbered like numberedLine ("1. e4", "c5", "2. Nf3"). */
  text: string;
  /** Always-numbered form for captions ("1. e4", "1... c5", "2. Nf3"). */
  label: string;
}

/** Per-move tokens of a SAN line starting from `fen` (pure numbering —
 * no legality; the texts joined by spaces equal numberedLine's output). */
export function scrubTokens(sans: string[], fen: string): ScrubToken[] {
  const fields = fen.split(" ");
  let whiteToMove = (fields[1] ?? "w") !== "b";
  let moveNo = Number.parseInt(fields[5] ?? "1", 10) || 1;
  return sans.map((san, i) => {
    const label = whiteToMove ? `${moveNo}. ${san}` : `${moveNo}... ${san}`;
    const text = whiteToMove || i === 0 ? label : san;
    if (!whiteToMove) moveNo += 1;
    whiteToMove = !whiteToMove;
    return { text, label };
  });
}

interface ScrubLineProps {
  /** SAN moves of the line, from `startFen` (default: standard start). */
  sans: string[];
  /** Position the line starts from (engine extensions start mid-game). */
  startFen?: string;
  /** Preview sink; null = no preview (mouse left / cleared). */
  onPreview: (p: ScrubPreview | null) => void;
  className?: string;
  /** Dim the first N moves: the part this line shares with the trunk it
   * continues, so the eye lands on what is new. Still hoverable. */
  dimBefore?: number;
  /** Own the keyboard (focusable, arrow keys step the preview). Set false
   * inside an interactive parent — a focusable element nested in a button
   * is a keyboard trap, so there the line is hover-only and the parent
   * keeps its own keyboard behaviour. */
  focusable?: boolean;
}

export default function ScrubLine({
  sans,
  startFen,
  onPreview,
  className,
  dimBefore = 0,
  focusable = true,
}: ScrubLineProps) {
  // Latest-callback ref: hover handlers and the unmount cleanup never
  // capture a stale onPreview.
  const onPreviewRef = useRef(onPreview);
  onPreviewRef.current = onPreview;
  /** True while THIS line owns the live preview (set on hover, cleared
   * on null) — only then may unmount clear the shared preview state. */
  const ownsRef = useRef(false);
  /** Keyboard cursor: 0 = none, k = previewing after move k. */
  const [kbPly, setKbPly] = useState(0);

  // Precompute the whole replay once — the hover path is a pure lookup.
  const tokens = useMemo(() => scrubTokens(sans, startFen ?? START_FEN), [sans, startFen]);
  const game = useMemo(() => {
    if (sans.length === 0) return null;
    const r = gameFromSans(sans, startFen ?? null);
    // A truncated replay (illegal move mid-line) is as unusable as a
    // failed one: token i would preview the wrong position.
    return r.ok && r.game.sans.length === sans.length ? r.game : null;
  }, [sans, startFen]);

  useEffect(
    () => () => {
      if (ownsRef.current) onPreviewRef.current(null);
    },
    [],
  );

  const plainText = tokens.map((t) => t.text).join(" ");
  if (!game) {
    return <span className={className}>{plainText}</span>;
  }

  const fire = (ply: number) => {
    if (ply <= 0) {
      ownsRef.current = false;
      onPreviewRef.current(null);
      return;
    }
    ownsRef.current = true;
    onPreviewRef.current({
      fen: game.fens[ply],
      lastMove: lastMoveAt(game, ply) ?? null,
      ply,
      label: tokens[ply - 1].label,
    });
  };

  const clear = () => {
    setKbPly(0);
    fire(0);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    let next: number;
    if (e.key === "ArrowRight") next = Math.min(sans.length, kbPly + 1);
    else if (e.key === "ArrowLeft") next = Math.max(0, kbPly - 1);
    else if (e.key === "Home") next = 1;
    else if (e.key === "End") next = sans.length;
    else if (e.key === "Escape" && kbPly > 0) next = 0;
    else return;
    e.preventDefault();
    setKbPly(next);
    fire(next);
  };

  return (
    <span
      className={`scrub-line${className ? ` ${className}` : ""}`}
      tabIndex={focusable ? 0 : undefined}
      aria-label={
        focusable
          ? `Line ${plainText}. Hover or use arrow keys to preview the moves on the board.`
          : undefined
      }
      onMouseLeave={clear}
      onBlur={focusable ? clear : undefined}
      onKeyDown={focusable ? onKeyDown : undefined}
    >
      {tokens.map((t, i) => (
        <span key={i}>
          {i > 0 ? " " : ""}
          <span
            className={`scrub-tok${kbPly === i + 1 ? " cur" : ""}${
              i < dimBefore ? " shared" : ""
            }`}
            onMouseEnter={() => fire(i + 1)}
          >
            {t.text}
          </span>
        </span>
      ))}
    </span>
  );
}
