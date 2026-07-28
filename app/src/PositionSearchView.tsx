/**
 * Position search screen (design/handoff-2 §Position search, solid-
 * simplified): left 560px desk column with a drag-to-set-up board editor
 * (chessground free mode), FEN field + Paste FEN + hint line; right
 * results column with the mono "N GAMES · X ms" pill (measured timing —
 * a product claim, never estimated) and the Database table minus the
 * analysis column.
 *
 * Result filters (run 10): Elo (bounds the game's HIGHER rating — unrated
 * games drop out when a bound is set), date range (permissive on PGN "?"
 * wildcards) and result, applied backend-side so the "N GAMES" pill counts
 * the filtered set. The hits list shows white/black/event/date/ply — the
 * position index carries no source or dup data (stated honestly).
 */
import { useCallback, useEffect, useState } from "react";
import { parseFen } from "chessops/fen";
import Board from "./Board";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import ScreenHeader from "./shell/ScreenHeader";
import { findGamesAt, type GameAtRow, type GamesAt, type GamesAtFilter } from "./lib/db";
import type { BoardTreatment } from "./lib/evidence";
import { dateBoundParam, dateRangeHint, eloParam } from "./lib/filters";
import { fenFromPlacement, placementOf, turnOf } from "./lib/search";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const EMPTY_FEN = "8/8/8/8/8/8/8/8 w - - 0 1";
const GRID = "1.5fr 1.5fr 58px 1fr 92px 64px";

interface PositionSearchViewProps {
  dbOpen: boolean;
  treatment: BoardTreatment;
  /** Open a database game at the ply that reached the position. */
  onOpenGameAt: (gameId: number, ply: number) => void;
}

export default function PositionSearchView({
  dbOpen,
  treatment,
  onOpenGameAt,
}: PositionSearchViewProps) {
  const [fen, setFen] = useState(START_FEN);
  const [fenField, setFenField] = useState(START_FEN);
  const [fenError, setFenError] = useState<string | null>(null);
  const [results, setResults] = useState<GamesAt | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Result filters (run 10) — plain input strings; invalid values are
  // held back from the backend rather than erroring per keystroke.
  const [minElo, setMinElo] = useState("");
  const [maxElo, setMaxElo] = useState("");
  const [dateMin, setDateMin] = useState("");
  const [dateMax, setDateMax] = useState("");
  const [resultFilter, setResultFilter] = useState("");

  const turn = turnOf(fen);

  // Search on every position/filter change (debounced — dragging fires often).
  useEffect(() => {
    if (!dbOpen) return;
    let cancelled = false;
    const filter: GamesAtFilter = {
      minElo: eloParam(minElo),
      maxElo: eloParam(maxElo),
      dateMin: dateBoundParam(dateMin),
      dateMax: dateBoundParam(dateMax),
      result: resultFilter || undefined,
    };
    const t = setTimeout(() => {
      findGamesAt(fen, filter)
        .then((r) => {
          if (cancelled) return;
          setResults(r);
          setError(null);
        })
        .catch((e) => !cancelled && setError(String(e)));
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [dbOpen, fen, minElo, maxElo, dateMin, dateMax, resultFilter]);

  const applyFen = useCallback((next: string) => {
    setFen(next);
    setFenField(next);
    setFenError(null);
  }, []);

  /** Board edit: keep the chosen side to move, derive castling honestly. */
  const onBoardChange = useCallback(
    (placement: string) => {
      applyFen(fenFromPlacement(placement, turn));
    },
    [applyFen, turn],
  );

  const commitFenField = useCallback(() => {
    const text = fenField.trim();
    const parsed = parseFen(text);
    if (parsed.isErr) {
      setFenError(`Not a valid FEN: ${parsed.error.message}`);
      return;
    }
    applyFen(text);
  }, [fenField, applyFen]);

  const pasteFen = useCallback(async () => {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      const parsed = parseFen(text);
      if (parsed.isErr) {
        setFenError(`Clipboard is not a FEN: ${parsed.error.message}`);
        return;
      }
      applyFen(text);
    } catch {
      setFenError("Clipboard unavailable.");
    }
  }, [applyFen]);

  const setTurn = useCallback(
    (t: "white" | "black") => {
      applyFen(fenFromPlacement(placementOf(fen), t));
    },
    [fen, applyFen],
  );

  const columns: DataTableColumn<GameAtRow>[] = [
    { key: "white", header: "WHITE", render: (g) => g.white },
    { key: "black", header: "BLACK", render: (g) => g.black },
    { key: "result", header: "RES", render: (g) => <span className="cell-result">{g.result}</span> },
    { key: "event", header: "EVENT", render: (g) => <span className="cell-dim">{g.event}</span> },
    {
      key: "date",
      header: "DATE",
      render: (g) => <span className="cell-date">{g.date}</span>,
      sort: (a, b) => a.date.localeCompare(b.date),
    },
    {
      key: "ply",
      header: "PLY",
      align: "right",
      render: (g) => <span className="cell-mono">{g.ply}</span>,
      sort: (a, b) => a.ply - b.ply,
    },
  ];

  const pill =
    results !== null
      ? `${results.total.toLocaleString("en-US")} GAMES · ${
          results.elapsedMs < 1 ? "<1" : Math.round(results.elapsedMs)
        } ms`
      : null;

  return (
    <>
      <ScreenHeader
        title="Position search"
        subtitle="Which games reached this position — drag pieces to set it up"
        actions={
          <>
            <button className="btn-secondary" onClick={() => applyFen(START_FEN)}>
              Start position
            </button>
            <button className="btn-secondary" onClick={() => applyFen(EMPTY_FEN)}>
              Clear board
            </button>
          </>
        }
      />
      <div className="search-layout">
        <div className="search-desk">
          <Board fen={fen} treatment={treatment} size={472} free={{ onChange: onBoardChange }} />
          <div className="search-fen-row">
            <input
              className="search-fen-input"
              type="text"
              value={fenField}
              onChange={(e) => setFenField(e.target.value)}
              onBlur={commitFenField}
              onKeyDown={(e) => e.key === "Enter" && commitFenField()}
              spellCheck={false}
            />
            <button className="btn-secondary" onClick={() => void pasteFen()}>
              Paste FEN
            </button>
          </div>
          <div className="search-turn-row">
            <div className="seg">
              <button className={turn === "white" ? "cur" : ""} onClick={() => setTurn("white")}>
                White to move
              </button>
              <button className={turn === "black" ? "cur" : ""} onClick={() => setTurn("black")}>
                Black to move
              </button>
            </div>
          </div>
          {fenError && <div className="error">{fenError}</div>}
          <div className="search-hint">
            Set the position by dragging pieces (drop off the board to remove). Castling rights
            follow king and rook home squares.
          </div>
        </div>
        <div className="search-results">
          <div className="search-results-head">
            <span className="aside-title">RESULTS</span>
            {pill && <span className="home-pill">{pill}</span>}
            <div className="filter-spacer" />
            <input
              className="filter-chip-input elo"
              type="text"
              inputMode="numeric"
              value={minElo}
              onChange={(e) => setMinElo(e.target.value)}
              placeholder="Elo ≥"
              title="Bounds the game's higher rating; unrated games drop out when set"
              spellCheck={false}
            />
            <input
              className="filter-chip-input elo"
              type="text"
              inputMode="numeric"
              value={maxElo}
              onChange={(e) => setMaxElo(e.target.value)}
              placeholder="Elo ≤"
              title="Bounds the game's higher rating; unrated games drop out when set"
              spellCheck={false}
            />
            <input
              className="filter-chip-input date"
              type="text"
              value={dateMin}
              onChange={(e) => setDateMin(e.target.value)}
              placeholder="From YYYY[.MM[.DD]]"
              title="Inclusive lower date bound (wildcard dates match permissively)"
              spellCheck={false}
            />
            <input
              className="filter-chip-input date"
              type="text"
              value={dateMax}
              onChange={(e) => setDateMax(e.target.value)}
              placeholder="To YYYY[.MM[.DD]]"
              title="Inclusive upper date bound"
              spellCheck={false}
            />
            <select
              className="filter-chip-select"
              value={resultFilter}
              onChange={(e) => setResultFilter(e.target.value)}
            >
              <option value="">Result</option>
              <option value="1-0">1-0</option>
              <option value="0-1">0-1</option>
              <option value="1/2-1/2">½-½</option>
              <option value="*">*</option>
            </select>
          </div>
          {dateRangeHint(dateMin, dateMax) && (
            <div className="filter-hint">{dateRangeHint(dateMin, dateMax)}</div>
          )}
          {error && <div className="error">{error}</div>}
          {results && (
            <>
              <DataTable
                columns={columns}
                rows={results.rows}
                gridTemplate={GRID}
                rowKey={(g) => g.id}
                onRowClick={(g) => onOpenGameAt(g.id, g.ply)}
                empty={dbOpen ? "No games reach this position." : "Open a database first."}
                footer={
                  results.total > results.rows.length ? (
                    <div className="pager-row">
                      <span className="pager-note">
                        Showing the first {results.rows.length} of{" "}
                        {results.total.toLocaleString("en-US")} hits.
                      </span>
                    </div>
                  ) : undefined
                }
              />
            </>
          )}
        </div>
      </div>
    </>
  );
}
