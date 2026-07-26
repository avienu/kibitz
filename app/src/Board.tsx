import { useEffect, useRef } from "react";
import { Chessground } from "chessground";
import type { Api } from "chessground/api";
import type { Key } from "chessground/types";

interface BoardProps {
  fen: string;
  lastMove?: [string, string];
}

/** View-only chessground board that tracks the `fen` prop. */
export default function Board({ fen, lastMove }: BoardProps) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const apiRef = useRef<Api | null>(null);

  useEffect(() => {
    if (!elRef.current) return;
    apiRef.current = Chessground(elRef.current, {
      fen,
      viewOnly: true,
      coordinates: true,
      animation: { enabled: true, duration: 150 },
      lastMove: lastMove as Key[] | undefined,
    });
    return () => {
      apiRef.current?.destroy();
      apiRef.current = null;
    };
    // Mount once; subsequent updates go through .set() below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    apiRef.current?.set({ fen, lastMove: lastMove as Key[] | undefined });
  }, [fen, lastMove]);

  return <div className="board" ref={elRef} />;
}
