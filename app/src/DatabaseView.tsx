import { useCallback, useEffect, useState } from "react";
import {
  findGamesAt,
  getGame,
  getSavedDbPath,
  listGames,
  openDatabase,
  openingTree,
  saveDbPath,
  type DbSummary,
  type GameDetail,
  type GameList,
  type GamesAt,
  type TreeRow,
} from "./lib/db";
import type { LoadedGame } from "./lib/game";

const PAGE_SIZE = 50;

interface DatabaseViewProps {
  /** FEN currently displayed on the board (drives the opening tree). */
  currentFen: string;
  game: LoadedGame | null;
  ply: number;
  /** Called with a fetched game; the parent builds the board model. */
  onLoadGame: (detail: GameDetail) => void;
  /** Advance the game view by one mainline ply. */
  onAdvance: () => void;
}

/** Strip decorations so tree SANs compare against game SANs. */
function normSan(san: string): string {
  return san.replace(/[!?]+$/, "");
}

function fmtElo(elo: number | null): string {
  return elo === null ? "" : String(elo);
}

/** Right-hand panel: open a database, browse/filter games, opening tree. */
export default function DatabaseView({
  currentFen,
  game,
  ply,
  onLoadGame,
  onAdvance,
}: DatabaseViewProps) {
  const [path, setPath] = useState(getSavedDbPath);
  const [summary, setSummary] = useState<DbSummary | null>(null);
  const [opening, setOpening] = useState(false);
  const [dbError, setDbError] = useState<string | null>(null);

  const [playerFilter, setPlayerFilter] = useState("");
  const [page, setPage] = useState(0);
  const [list, setList] = useState<GameList | null>(null);
  const [listError, setListError] = useState<string | null>(null);

  const [tree, setTree] = useState<TreeRow[]>([]);
  const [treeHint, setTreeHint] = useState<string | null>(null);
  const [gamesAt, setGamesAt] = useState<GamesAt | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const doOpen = useCallback(async () => {
    setOpening(true);
    setDbError(null);
    try {
      const s = await openDatabase(path);
      setSummary(s);
      saveDbPath(path);
      setPage(0);
    } catch (e) {
      setSummary(null);
      setDbError(String(e));
    } finally {
      setOpening(false);
    }
  }, [path]);

  // Game list: refetch on open / filter (debounced) / page change.
  useEffect(() => {
    if (!summary) return;
    let cancelled = false;
    const t = setTimeout(
      () => {
        listGames({ playerSubstring: playerFilter || undefined }, page * PAGE_SIZE, PAGE_SIZE)
          .then((l) => {
            if (cancelled) return;
            setList(l);
            setListError(null);
          })
          .catch((e) => !cancelled && setListError(String(e)));
      },
      playerFilter ? 250 : 0,
    );
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [summary, playerFilter, page]);

  // Opening tree + games-at-position: refetch whenever the shown FEN changes.
  useEffect(() => {
    if (!summary) return;
    let cancelled = false;
    setTreeHint(null);
    openingTree(currentFen)
      .then((rows) => !cancelled && setTree(rows))
      .catch(() => !cancelled && setTree([]));
    findGamesAt(currentFen)
      .then((g) => !cancelled && setGamesAt(g))
      .catch(() => !cancelled && setGamesAt(null));
    return () => {
      cancelled = true;
    };
  }, [summary, currentFen]);

  const loadById = useCallback(
    async (id: number) => {
      try {
        const detail = await getGame(id);
        setSelectedId(id);
        onLoadGame(detail);
      } catch (e) {
        setListError(String(e));
      }
    },
    [onLoadGame],
  );

  const onTreeRowClick = (row: TreeRow) => {
    const next = game && ply < game.sans.length ? game.sans[ply] : null;
    if (next !== null && normSan(next) === normSan(row.san)) {
      onAdvance();
    } else if (next === null) {
      setTreeHint(
        game
          ? `End of the loaded game — ${row.san} was played here in other games.`
          : `Load a game to step through it; ${row.san} was played here in ${row.count} game(s).`,
      );
    } else {
      setTreeHint(`The loaded game continues ${next} here, not ${row.san}.`);
    }
  };

  const totalPages = list ? Math.max(1, Math.ceil(list.total / PAGE_SIZE)) : 1;

  return (
    <div className="dbview">
      <div className="db-section">
        <h3>Database</h3>
        <div className="db-open-row">
          <input
            type="text"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="path to .sqlite database"
            spellCheck={false}
          />
          <button onClick={() => void doOpen()} disabled={opening}>
            {opening ? "Opening…" : "Open"}
          </button>
        </div>
        {dbError && <div className="error">{dbError}</div>}
        {summary && (
          <div className="db-summary">
            {summary.games.toLocaleString()} games, {summary.players.toLocaleString()} players,{" "}
            {summary.positions.toLocaleString()} positions, {summary.sources} source
            {summary.sources === 1 ? "" : "s"} — {summary.path}
          </div>
        )}
      </div>

      {summary && (
        <div className="db-section">
          <h3>Opening tree (current position)</h3>
          {tree.length === 0 ? (
            <div className="db-empty">No database moves from this position.</div>
          ) : (
            <table className="tree-table">
              <thead>
                <tr>
                  <th>Move</th>
                  <th className="num">Games</th>
                  <th>W / D / L</th>
                  <th className="num">Elo</th>
                  <th className="num">Perf</th>
                </tr>
              </thead>
              <tbody>
                {tree.map((row) => {
                  const n = Math.max(1, row.count);
                  return (
                    <tr key={row.san} onClick={() => onTreeRowClick(row)}>
                      <td className="san">{row.san}</td>
                      <td className="num">{row.count.toLocaleString()}</td>
                      <td>
                        <div className="wdl" title={`+${row.whiteWins} =${row.draws} -${row.blackWins}`}>
                          <span className="w" style={{ width: `${(100 * row.whiteWins) / n}%` }} />
                          <span className="d" style={{ width: `${(100 * row.draws) / n}%` }} />
                          <span className="b" style={{ width: `${(100 * row.blackWins) / n}%` }} />
                        </div>
                      </td>
                      <td className="num">{fmtElo(row.avgElo)}</td>
                      <td className="num">{fmtElo(row.perf)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
          {treeHint && <div className="hint">{treeHint}</div>}
          {gamesAt && (
            <div className="games-at">
              <div className="games-at-count">
                {gamesAt.total.toLocaleString()} game{gamesAt.total === 1 ? "" : "s"} reach
                {gamesAt.total === 1 ? "es" : ""} this position
              </div>
              {gamesAt.rows.slice(0, 10).map((g) => (
                <div key={g.id} className="games-at-row">
                  <span className="ga-players">
                    {g.white} — {g.black}
                  </span>
                  <span className="ga-meta">
                    {g.result} {g.date} (ply {g.ply})
                  </span>
                  <button onClick={() => void loadById(g.id)}>load</button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {summary && (
        <div className="db-section">
          <h3>Games</h3>
          <div className="db-filter-row">
            <input
              type="text"
              value={playerFilter}
              onChange={(e) => {
                setPlayerFilter(e.target.value);
                setPage(0);
              }}
              placeholder="filter by player name…"
              spellCheck={false}
            />
            {list && <span className="db-total">{list.total.toLocaleString()} games</span>}
          </div>
          {listError && <div className="error">{listError}</div>}
          {list && (
            <>
              <table className="games-table">
                <thead>
                  <tr>
                    <th>White</th>
                    <th className="num">Elo</th>
                    <th>Black</th>
                    <th className="num">Elo</th>
                    <th>Result</th>
                    <th>Date</th>
                    <th>ECO</th>
                    <th>Event</th>
                  </tr>
                </thead>
                <tbody>
                  {list.rows.map((g) => (
                    <tr
                      key={g.id}
                      className={g.id === selectedId ? "sel" : ""}
                      onClick={() => void loadById(g.id)}
                    >
                      <td>{g.white}</td>
                      <td className="num">{fmtElo(g.whiteElo)}</td>
                      <td>{g.black}</td>
                      <td className="num">{fmtElo(g.blackElo)}</td>
                      <td>{g.result}</td>
                      <td>{g.date ?? ""}</td>
                      <td>{g.eco ?? ""}</td>
                      <td className="ev">{g.event}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="pager">
                <button onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0}>
                  ◀ Prev
                </button>
                <span>
                  page {page + 1} / {totalPages}
                </span>
                <button
                  onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                  disabled={page + 1 >= totalPages}
                >
                  Next ▶
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
