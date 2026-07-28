import { useEffect, useMemo, useRef } from "react";
import type { CSSProperties } from "react";
import { Chessground } from "chessground";
import type { Api } from "chessground/api";
import type { DrawBrushes, DrawShape } from "chessground/draw";
import type { Key } from "chessground/types";
import { fenTurn } from "./lib/fen";
import {
  EVIDENCE_COLORS,
  arrowPolygonPoints,
  arrowStrokeWidth,
  boardGeometry,
  evidenceView,
  type ArrowKind,
  type BoardShape,
  type BoardTreatment,
  type Evidence,
  type SquareMark,
} from "./lib/evidence";

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
  /** Raw auto-shapes (trainer arrows etc.). Ignored while `evidence` is set. */
  shapes?: BoardShape[];
  /** Bottom side of the board (default white; Train flips for Black). */
  orientation?: "white" | "black";
  /** Board skin (design/handoff-1): Studio Walnut default, Instrument alternate. */
  treatment?: BoardTreatment;
  /** Grid edge in px, snapped to a multiple of 8 (seam-free). Default 520. */
  size?: number;
  /** Evidence-overlay input — the one shared overlay language. */
  evidence?: Evidence | null;
  /** Overlay loudness: 0.44 baseline, 1.0 for the hovered sentence. */
  intensity?: number;
  /** Per-block isolation: restrict evidence to these squares. */
  isolate?: ReadonlySet<string>;
  /** Free set-up mode (Position search's drag-to-set-up editor): any piece
   * drags anywhere, dropping off the board deletes it, and every change
   * reports the new placement (the FEN board field). Overrides `movable`. */
  free?: { onChange: (placement: string) => void };
}

/** Percent position of a square in the visual grid for an orientation. */
function squarePos(square: string, orientation: "white" | "black"): CSSProperties {
  const f = square.charCodeAt(0) - 97; // a..h
  const r = square.charCodeAt(1) - 49; // 1..8
  const x = orientation === "black" ? 7 - f : f;
  const y = orientation === "black" ? r : 7 - r;
  return { left: `${x * 12.5}%`, top: `${y * 12.5}%` };
}

/** 64 static texture cells (walnut grain / instrument seam, via CSS).
 * Square-colour parity is symmetric under flipping, so this never re-renders. */
const TEXTURE_CELLS = (() => {
  const cells: { key: string; className: string; style: CSSProperties }[] = [];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 8; x++) {
      const dark = (x + y) % 2 === 1; // top-left (a8 from White) is light
      cells.push({
        key: `t${x}${y}`,
        className: `kibitz-sq ${dark ? "kibitz-sq-dark" : "kibitz-sq-light"}`,
        style: { left: `${x * 12.5}%`, top: `${y * 12.5}%` },
      });
    }
  }
  return cells;
})();

/** Chessground board that tracks the `fen` prop; optionally accepts move
 * input (a played move snaps back — the model decides what it means).
 * Skinned per design/handoff-1; evidence overlays render as an absolutely
 * positioned mark grid (under pieces) plus chessground auto-shape arrows. */
export default function Board({
  fen,
  lastMove,
  movable,
  shapes,
  orientation,
  treatment = "walnut",
  size = 520,
  evidence,
  intensity,
  isolate,
  free,
}: BoardProps) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const apiRef = useRef<Api | null>(null);
  const fenRef = useRef(fen);
  const lastMoveRef = useRef(lastMove);
  const onMoveRef = useRef(movable?.onMove);
  const freeRef = useRef(free);
  fenRef.current = fen;
  lastMoveRef.current = lastMove;
  onMoveRef.current = movable?.onMove;
  freeRef.current = free;

  const geo = boardGeometry(size, treatment);
  const view = useMemo(
    () => evidenceView(evidence ?? null, { intensity, isolate, lastMove }),
    [evidence, intensity, isolate, lastMove],
  );

  useEffect(() => {
    if (!elRef.current) return;
    apiRef.current = Chessground(elRef.current, {
      fen,
      turnColor: fenTurn(fen),
      orientation: orientation ?? "white",
      coordinates: true,
      animation: { enabled: true, duration: 150 },
      lastMove: lastMove as Key[] | undefined,
      // The last-move wash is drawn by the evidence overlay (exact colours).
      highlight: { lastMove: false, check: true },
      // In free mode, report every position change (drag, drop-off delete).
      events: {
        change: () => {
          const api = apiRef.current;
          if (freeRef.current && api) freeRef.current.onChange(api.getFen());
        },
      },
      draggable: { deleteOnDropOff: !!free },
      // The app never uses premoves; without this, a mismatched turnColor
      // turns user moves into silently-queued premoves (purple squares).
      premovable: { enabled: false },
      movable: {
        free: !!free,
        color: free ? "both" : undefined,
        showDests: true,
        events: {
          after: (orig, dest) => {
            if (freeRef.current) return; // free mode: the board IS the model
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
        // Partial brush set: chessground deep-merges it over the defaults —
        // "orange" for the legacy trainer shapes. (Evidence arrows are NOT
        // chessground shapes — the overlay's own SVG layer draws them.)
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
      turnColor: fenTurn(fen),
      orientation: orientation ?? "white",
      lastMove: lastMove as Key[] | undefined,
      movable: free
        ? { free: true, color: "both", dests: new Map() }
        : movable
          ? { free: false, color: movable.color, dests: movable.dests as Map<Key, Key[]> }
          : { free: false, color: undefined, dests: new Map() },
    });
  }, [fen, lastMove, movable, orientation, free]);

  // Legacy trainer shapes only — evidence arrows never go through chessground.
  useEffect(() => {
    apiRef.current?.setAutoShapes((shapes ?? []) as DrawShape[]);
  }, [shapes]);

  const styleVars = {
    "--sb-size": `${geo.size}px`,
    "--sb-cell": `${geo.cell}px`,
    "--sb-frame-pad": `${geo.framePad}px`,
    "--sb-gutter": `${geo.gutter}px`,
    "--sb-coord-fs": `${geo.coordFontSize}px`,
    "--sb-coord-inset-b": `${geo.coordInsetBottom}px`,
    "--sb-coord-inset-l": `${geo.coordInsetLeft}px`,
  } as CSSProperties;

  const markEl = (m: SquareMark) => (
    <div
      key={`${m.square}-${m.role}`}
      className={`kibitz-mark kibitz-mark-${m.role}`}
      style={{ ...squarePos(m.square, orientation ?? "white"), opacity: m.opacity }}
    />
  );

  return (
    <div className={`board kibitz-board kibitz-${treatment}`} style={styleVars}>
      <div className="kibitz-grid" style={{ width: geo.size, height: geo.size }}>
        <div ref={elRef} style={{ width: geo.size, height: geo.size }} />
        <div className="kibitz-overlay" aria-hidden>
          {TEXTURE_CELLS.map((c) => (
            <div key={c.key} className={c.className} style={c.style} />
          ))}
          {view.marks.map(markEl)}
        </div>
        {view.shapes.length > 0 && (
          <svg
            className="kibitz-arrows"
            viewBox={`0 0 ${geo.size} ${geo.size}`}
            width={geo.size}
            height={geo.size}
            aria-hidden
          >
            {view.shapes.map((s) => (
              <polygon
                key={`${s.orig}-${s.dest}`}
                points={arrowPolygonPoints(s.orig, s.dest!, geo.cell, orientation ?? "white")}
                fill={EVIDENCE_COLORS[s.brush as ArrowKind].line}
                strokeWidth={arrowStrokeWidth(geo.cell)}
                opacity={view.arrowOpacity}
              />
            ))}
          </svg>
        )}
      </div>
    </div>
  );
}
