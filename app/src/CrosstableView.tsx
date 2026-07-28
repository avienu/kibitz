/**
 * Crosstable modal (run 10): players × rounds grid for one event,
 * computed from the games the database actually holds (lib/crosstable.ts
 * owns the layout math). Events without usable round data degrade to the
 * scored player list — points, games, performance — and say so.
 *
 * Opened from the game-view header's event line or a Database row's
 * event cell; clicking any result cell opens that game.
 */
import { useEffect, useState } from "react";
import {
  buildCrosstable,
  crosstableGames,
  formatPoints,
  type Crosstable,
} from "./lib/crosstable";

interface CrosstableViewProps {
  event: string;
  onClose: () => void;
  /** Open a game from a cell (the modal closes itself first). */
  onOpenGame: (gameId: number) => void;
}

export default function CrosstableView({ event, onClose, onOpenGame }: CrosstableViewProps) {
  const [table, setTable] = useState<Crosstable | null>(null);
  const [truncated, setTruncated] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    crosstableGames(event)
      .then((g) => {
        if (cancelled) return;
        setTable(buildCrosstable(g.rows));
        setTruncated(g.total > g.rows.length ? g.total : null);
        setError(null);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [event]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const cellButton = (
    c: { gameId: number; opponent: string; score: string; color: "w" | "b" },
    key: number,
  ) => (
    <button
      key={key}
      className={`xt-cell xt-${c.score === "1" ? "win" : c.score === "0" ? "loss" : "rest"}`}
      title={`${c.color === "w" ? "White" : "Black"} vs ${c.opponent} — click to open the game`}
      onClick={() => onOpenGame(c.gameId)}
    >
      {c.score}
      <span className="xt-cell-opp">{c.opponent.split(",")[0]}</span>
    </button>
  );

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal xt-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title xt-title">
          <span>Crosstable — {event}</span>
          {table && (
            <span className="xt-meta">
              {table.games.toLocaleString("en-US")} game{table.games === 1 ? "" : "s"} ·{" "}
              {table.players.length} player{table.players.length === 1 ? "" : "s"}
            </span>
          )}
        </div>

        {error && <div className="error">Crosstable failed: {error}</div>}
        {!table && !error && <div className="xt-note">Loading…</div>}

        {table && table.games === 0 && (
          <p className="xt-note">No games under this event name in the open database.</p>
        )}

        {table && table.games > 0 && (
          <div className="xt-scroll">
            <table className="xt-table">
              <thead>
                <tr>
                  <th className="xt-rank">#</th>
                  <th className="xt-player">PLAYER</th>
                  <th>ELO</th>
                  <th>PTS</th>
                  <th>GAMES</th>
                  <th title="avg opponent Elo + 800·score − 400, finished games vs rated opponents">
                    PERF
                  </th>
                  {table.mode === "grid" &&
                    table.rounds.map((r) => <th key={r}>R{r}</th>)}
                  {table.mode === "grid" && table.hasUnrounded && <th title="games whose Round tag could not be parsed">?</th>}
                </tr>
              </thead>
              <tbody>
                {table.players.map((p, i) => (
                  <tr key={p.name}>
                    <td className="xt-rank">{i + 1}</td>
                    <td className="xt-player">{p.name}</td>
                    <td>{p.elo ?? ""}</td>
                    <td className="xt-pts">
                      {formatPoints(p.points)}
                      <span className="xt-of">/{p.counted}</span>
                    </td>
                    <td>{p.games}</td>
                    <td>{p.perf ?? "—"}</td>
                    {table.mode === "grid" &&
                      p.cells.map((cell, j) => (
                        <td key={j} className="xt-round-cell">
                          {cell.map((c, k) => cellButton(c, k))}
                        </td>
                      ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {table && table.mode === "list" && table.games > 0 && (
          <p className="xt-note">
            Most of this event's games carry no usable Round tag, so there is no honest
            rounds grid — this is the scored player list instead.
          </p>
        )}
        {truncated !== null && (
          <p className="xt-note">
            Showing the first {table?.games.toLocaleString("en-US")} of{" "}
            {truncated.toLocaleString("en-US")} games filed under this event name.
          </p>
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
