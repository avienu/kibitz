/**
 * Opening tree screen (design/handoff-2 §Opening tree, solid-simplified):
 * left move table (shared DataTable: move | games | W/D/L stacked bar |
 * avg Elo | perf) + right 520px aside with the board following the
 * displayed position, the "Games reaching this position" list and one
 * serif transposition paragraph. The measured query time shows in the
 * moves line — a real number, never estimated.
 *
 * PERF shows the performance rating's signed delta against the movers'
 * average Elo (both values are real; the delta is their honest headline).
 */
import { useEffect, useMemo, useState } from "react";
import Board from "./Board";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import ScreenHeader from "./shell/ScreenHeader";
import { findGamesAt, openingTree, type GamesAt, type TreeRow } from "./lib/db";
import { gameFromSans, lastMoveAt } from "./lib/game";
import { reachingEmptyCopy, treeEmptyCopy, treePhase } from "./lib/treeView";
import type { BoardTreatment } from "./lib/evidence";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const GRID = "74px 84px 1fr 78px 74px";

interface OpeningTreeViewProps {
  dbOpen: boolean;
  treatment: BoardTreatment;
  /** Open a database game at a ply (games-reaching row click). */
  onOpenGameAt: (gameId: number, ply: number) => void;
}

/** "1. e4 e5 2. Nf3" prefix labels for a SAN line. */
function numbered(line: readonly string[]): { san: string; label: string }[] {
  return line.map((san, i) => ({
    san,
    label: i % 2 === 0 ? `${i / 2 + 1}. ${san}` : san,
  }));
}

function wdlCell(row: TreeRow) {
  const n = Math.max(1, row.count);
  return (
    <span className="wdl-bar" title={`+${row.whiteWins} =${row.draws} -${row.blackWins}`}>
      <span className="wdl-w" style={{ width: `${(100 * row.whiteWins) / n}%` }} />
      <span className="wdl-d" style={{ width: `${(100 * row.draws) / n}%` }} />
      <span className="wdl-l" style={{ width: `${(100 * row.blackWins) / n}%` }} />
    </span>
  );
}

function perfCell(row: TreeRow) {
  if (row.perf == null || row.avgElo == null) return "";
  const delta = row.perf - row.avgElo;
  const cls = delta >= 0 ? "perf-pos" : "perf-neg";
  return (
    <span className={cls} title={`performance ${row.perf} vs avg ${row.avgElo}`}>
      {delta >= 0 ? `+${delta}` : String(delta)}
    </span>
  );
}

export default function OpeningTreeView({ dbOpen, treatment, onOpenGameAt }: OpeningTreeViewProps) {
  const [line, setLine] = useState<string[]>([]);
  const [rows, setRows] = useState<TreeRow[]>([]);
  const [elapsedMs, setElapsedMs] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [gamesAt, setGamesAt] = useState<GamesAt | null>(null);
  const [gamesAtLoading, setGamesAtLoading] = useState(false);
  const [gamesAtError, setGamesAtError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The displayed position is derived from the clicked-through line.
  const derived = useMemo(() => {
    if (line.length === 0) return { fen: START_FEN, lastMove: undefined as [string, string] | undefined };
    const res = gameFromSans(line, null, {});
    if (!res.ok) return { fen: START_FEN, lastMove: undefined };
    const g = res.game;
    return {
      fen: g.fens[g.fens.length - 1],
      lastMove: lastMoveAt(g, g.sans.length),
    };
  }, [line]);

  useEffect(() => {
    if (!dbOpen) return;
    let cancelled = false;
    // Loading is a real, named state (audit #2): a pending query must
    // never render the true-empty "No database moves…" copy, and a real
    // error must render as an error — the two were indistinguishable.
    setLoading(true);
    setGamesAtLoading(true);
    openingTree(derived.fen)
      .then((t) => {
        if (cancelled) return;
        setRows(t.rows);
        setElapsedMs(t.elapsedMs);
        setError(null);
        setLoading(false);
      })
      .catch((e) => {
        if (!cancelled) {
          setRows([]);
          setElapsedMs(null);
          setError(String(e));
          setLoading(false);
        }
      });
    findGamesAt(derived.fen)
      .then((g) => {
        if (cancelled) return;
        setGamesAt(g);
        setGamesAtError(null);
        setGamesAtLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setGamesAt(null);
        setGamesAtError(String(e));
        setGamesAtLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [dbOpen, derived.fen]);

  const columns: DataTableColumn<TreeRow>[] = [
    { key: "move", header: "MOVE", render: (r) => <span className="tree-move">{r.san}</span> },
    {
      key: "games",
      header: "GAMES",
      render: (r) => <span className="cell-mono">{r.count.toLocaleString("en-US")}</span>,
      sort: (a, b) => a.count - b.count,
    },
    { key: "wdl", header: "W / D / L", render: wdlCell },
    {
      key: "elo",
      header: "AVG ELO",
      render: (r) => <span className="cell-mono">{r.avgElo ?? ""}</span>,
      sort: (a, b) => (a.avgElo ?? 0) - (b.avgElo ?? 0),
    },
    { key: "perf", header: "PERF", render: perfCell },
  ];

  const crumbs = numbered(line);
  const timing = elapsedMs !== null ? ` · ${elapsedMs < 1 ? "<1" : Math.round(elapsedMs)} ms` : "";

  return (
    <>
      <ScreenHeader
        title="Opening tree"
        subtitle={`Transposition-aware · follows the displayed position${timing}`}
        actions={
          line.length > 0 && (
            <button className="btn-secondary" onClick={() => setLine([])}>
              Back to start
            </button>
          )
        }
      />
      <div className="tree-layout">
        <div className="tree-main">
          <div className="tree-line">
            {crumbs.length === 0 ? (
              <span className="tree-line-start">Initial position</span>
            ) : (
              crumbs.map((c, i) => (
                <button
                  key={`${i}-${c.san}`}
                  className="tree-crumb"
                  title="Rewind to this move"
                  onClick={() => setLine(line.slice(0, i + 1))}
                >
                  {c.label}
                </button>
              ))
            )}
            <span className="tree-line-hint">follows the board</span>
          </div>
          {error && <div className="error">Opening tree failed: {error}</div>}
          <DataTable
            columns={columns}
            rows={rows}
            gridTemplate={GRID}
            rowKey={(r) => r.san}
            onRowClick={(r) => setLine([...line, r.san.replace(/[!?]+$/, "")])}
            empty={treeEmptyCopy(treePhase(dbOpen, loading, error))}
          />
          <p className="tree-prose">
            Transposition-aware: these counts merge every move order that reaches this
            position, so a move here includes games that arrived by a different sequence.
          </p>
        </div>
        <aside className="tree-aside">
          <Board fen={derived.fen} lastMove={derived.lastMove} treatment={treatment} size={472} />
          <div>
            <div className="aside-title">GAMES REACHING THIS POSITION</div>
            {gamesAt && gamesAt.rows.length > 0 ? (
              <>
                <div className="reaching-list">
                  {gamesAt.rows.slice(0, 12).map((g) => (
                    <button
                      key={g.id}
                      className="reaching-row"
                      onClick={() => onOpenGameAt(g.id, g.ply)}
                    >
                      <span className="reaching-title">
                        {g.white} — {g.black}
                      </span>
                      <span className="reaching-meta">
                        {g.result}
                        {g.date ? ` · ${g.date}` : ""}
                      </span>
                    </button>
                  ))}
                </div>
                <div className="reaching-foot">
                  {gamesAt.total.toLocaleString("en-US")} game{gamesAt.total === 1 ? "" : "s"} total
                </div>
              </>
            ) : (
              <p className="tree-prose">
                {reachingEmptyCopy(treePhase(dbOpen, gamesAtLoading, gamesAtError))}
              </p>
            )}
          </div>
        </aside>
      </div>
    </>
  );
}
