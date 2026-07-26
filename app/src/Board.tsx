import { useEffect, useRef } from "react";
import { Chessground } from "chessground";
import type { Api } from "chessground/api";
import type { DrawBrushes, DrawShape } from "chessground/draw";
import type { Key } from "chessground/types";
import type { BoardShape } from "./lib/explainView";

export interface BoardMovable {
  /** Side allowed to input a move (the side to move). */
  color: "white" | "black";
  /** Legal destination squares per origin square. */
  dests: Map<string, string[]>;
  /** Called when the user completes a legal move on the board. */
  onMove: (orig: string, dest: string) => void;
}

interface BoardProps {
  fen: string;
  lastMove?: [string, string];
  /** When set, the side to move can input moves (annotation variations). */
  movable?: BoardMovable;
  /** Auto-shapes overlaid on the board (explain-position evidence). */
  shapes?: BoardShape[];
  /** Bottom side of the board (default white; Train flips for Black). */
  orientation?: "white" | "black";
}

/** Chessground board that tracks the `fen` prop; optionally accepts move
 * input (a played move snaps back — the model decides what it means). */
export default function Board({ fen, lastMove, movable, shapes, orientation }: BoardProps) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const apiRef = useRef<Api | null>(null);
  const fenRef = useRef(fen);
  const lastMoveRef = useRef(lastMove);
  const onMoveRef = useRef(movable?.onMove);
  fenRef.current = fen;
  lastMoveRef.current = lastMove;
  onMoveRef.current = movable?.onMove;

  useEffect(() => {
    if (!elRef.current) return;
    apiRef.current = Chessground(elRef.current, {
      fen,
      orientation: orientation ?? "white",
      coordinates: true,
      animation: { enabled: true, duration: 150 },
      lastMove: lastMove as Key[] | undefined,
      movable: {
        free: false,
        color: undefined,
        showDests: true,
        events: {
          after: (orig, dest) => {
            onMoveRef.current?.(orig, dest);
            // Snap back to the model position: if the move was accepted the
            // fen prop changes and the sync effect re-applies it anyway.
            requestAnimationFrame(() => {
              apiRef.current?.set({
                fen: fenRef.current,
                lastMove: lastMoveRef.current as Key[] | undefined,
              });
            });
          },
        },
      },
      drawable: {
        enabled: false,
        // Partial brush set: chessground deep-merges it over the defaults,
        // adding "orange" alongside the built-in red/green.
        brushes: {
          orange: { key: "or", color: "#e68f00", opacity: 0.9, lineWidth: 10 },
        } as unknown as DrawBrushes,
      },
    });
    return () => {
      apiRef.current?.destroy();
      apiRef.current = null;
    };
    // Mount once; subsequent updates go through .set() below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    apiRef.current?.set({
      fen,
      orientation: orientation ?? "white",
      lastMove: lastMove as Key[] | undefined,
      movable: movable
        ? { color: movable.color, dests: movable.dests as Map<Key, Key[]> }
        : { color: undefined, dests: new Map() },
    });
  }, [fen, lastMove, movable, orientation]);

  useEffect(() => {
    apiRef.current?.setAutoShapes((shapes ?? []) as DrawShape[]);
  }, [shapes]);

  return <div className="board" ref={elRef} />;
}
