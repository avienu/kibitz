/**
 * Opponent prep — round-2 build-out (design/handoff-2 §Screen: Opponent
 * prep). Four-step workflow in a header stepper strip (① Opponent →
 * ② Fingerprint → ③ Weak lines → ④ Master games) with free backward
 * navigation and persisted selections, beside a persistent 520px aside:
 * board with the position under discussion, a profile finding about this
 * opponent (honest absence otherwise), and the profile / game-view jumps.
 *
 * Engine-off stays visible: nothing here analyses anything.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import Board from "./Board";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import BaselineBar from "./components/BaselineBar";
import Stepper from "./components/Stepper";
import ScreenHeader from "./shell/ScreenHeader";
import {
  listGames,
  matchingPlayers,
  prepFingerprint,
  prepStateGet,
  prepStateSet,
  prepView,
  getGame,
  type FingerprintRow,
  type MasterGame,
  type PlayerProfile,
  type PrepFingerprint,
  type WeakLine,
} from "./lib/db";
import { gameFromSans } from "./lib/game";
import {
  MASTER_RANKING_RULE,
  bookExitFor,
  fingerprintRowWeak,
  lineMoves,
  lineName,
  lineScore,
  lineWhy,
  prepFinding,
  recordPrep,
  stepperSteps,
  type PrepColor,
} from "./lib/prepView";
import type { BoardTreatment } from "./lib/evidence";
import type { ViewId, ViewParams } from "./lib/shell";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

interface PrepViewProps {
  /** Load a database game into the game view at the given ply. */
  onLoadGameAt: (gameId: number, ply: number) => void;
  /** Profile built in the Profile view, if any (aside finding — used only
   * when it belongs to the selected opponent). */
  profile: PlayerProfile | null;
  /** Round-2 navigation contract (lib/shell.ts ViewParams): opponent name
   * to prefill step 1 — Home's "Prep an opponent" Go navigates here. */
  opponent?: string | null;
  /** Shell navigation ("Open his profile" → Profile, opponent subject). */
  onNavigate: (view: ViewId, params?: ViewParams) => void;
  /** Board skin for the aside board. */
  treatment?: BoardTreatment;
}

interface Candidate {
  name: string;
  /** Games in the local database involving the name (best-effort). */
  games: number | null;
}

export default function PrepView({
  onLoadGameAt,
  profile,
  opponent,
  onNavigate,
  treatment = "walnut",
}: PrepViewProps) {
  const [step, setStep] = useState(1);
  const [reached, setReached] = useState(1);
  const [query, setQuery] = useState(opponent ?? "");
  const [candidates, setCandidates] = useState<Candidate[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selOpponent, setSelOpponent] = useState<string | null>(null);
  const [color, setColor] = useState<PrepColor>("black");
  const [fp, setFp] = useState<PrepFingerprint | null>(null);
  const [lines, setLines] = useState<WeakLine[] | null>(null);
  const [loadingPrep, setLoadingPrep] = useState(false);
  const [selHash, setSelHash] = useState<string | null>(null);

  /** One prep_state record per opponent+color per mount. */
  const recordedRef = useRef<Set<string>>(new Set());

  /* ---- step 1: search ---- */
  const search = useCallback(async (name: string) => {
    const q = name.trim();
    if (q === "") return;
    setSearching(true);
    setError(null);
    setCandidates(null);
    try {
      const names = (await matchingPlayers(q)).slice(0, 8);
      const withCounts = await Promise.all(
        names.map(async (n) => {
          try {
            const list = await listGames({ playerSubstring: n }, 0, 1);
            return { name: n, games: list.total };
          } catch {
            return { name: n, games: null };
          }
        }),
      );
      setCandidates(withCounts);
    } catch (e) {
      setError(String(e));
    } finally {
      setSearching(false);
    }
  }, []);

  // Search as you type (run-8 user report): debounced live search once
  // two characters are in; Enter/the button still force an immediate run.
  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) return;
    const t = setTimeout(() => void search(query), 250);
    return () => clearTimeout(t);
  }, [query, search]);

  // Home's "Prep an opponent" prefills and searches immediately.
  const openedWithRef = useRef<string | null>(null);
  useEffect(() => {
    if (opponent && openedWithRef.current !== opponent) {
      openedWithRef.current = opponent;
      setQuery(opponent);
      void search(opponent);
    }
  }, [opponent, search]);

  /* ---- step 2+: fingerprint + weak lines (fetched together) ---- */
  // `prepStarted` is stable across later step advances — the fetch must
  // not re-fire (and wipe state) every time `reached` grows.
  const prepStarted = selOpponent !== null && reached >= 2;
  useEffect(() => {
    if (!selOpponent || !prepStarted) return;
    let stale = false;
    setLoadingPrep(true);
    setFp(null);
    setLines(null);
    setSelHash(null);
    setError(null);
    Promise.all([prepFingerprint(selOpponent, color), prepView(selOpponent, color)])
      .then(([f, l]) => {
        if (stale) return;
        setFp(f);
        setLines(l);
        setSelHash(l[0]?.hash ?? null);
      })
      .catch((e) => !stale && setError(String(e)))
      .finally(() => !stale && setLoadingPrep(false));
    return () => {
      stale = true;
    };
  }, [selOpponent, color, prepStarted]);

  // Record the started prep at step-2 entry (once per opponent+color) so
  // Home's "no prep started for X" stays truthful.
  useEffect(() => {
    if (!selOpponent || !prepStarted) return;
    const key = `${selOpponent}|${color}`;
    if (recordedRef.current.has(key)) return;
    recordedRef.current.add(key);
    prepStateGet()
      .then((entries) => prepStateSet(recordPrep(entries, selOpponent, color, new Date())))
      .catch(() => {}); // meta write is best-effort
  }, [selOpponent, color, prepStarted]);

  const goTo = (n: number) => {
    setStep(n);
    setReached((r) => Math.max(r, n));
  };

  const selectOpponent = (name: string) => {
    setSelOpponent(name);
    goTo(2);
  };

  const selLine = lines?.find((l) => l.hash === selHash) ?? lines?.[0] ?? null;

  const selectLine = (l: WeakLine) => {
    setSelHash(l.hash);
    goTo(4);
  };

  /* ---- aside board position (derived from a master game at the ply) ---- */
  const [fenCache, setFenCache] = useState<Map<string, string>>(new Map());
  const fenCacheRef = useRef(fenCache);
  fenCacheRef.current = fenCache;
  useEffect(() => {
    if (!selLine || selLine.masterGames.length === 0) return;
    if (fenCacheRef.current.has(selLine.hash)) return;
    const m = selLine.masterGames[0];
    let stale = false;
    getGame(m.gameId)
      .then((d) => {
        if (stale) return;
        const res = gameFromSans(d.sans, d.startFen);
        if (!res.ok) return;
        const fen = res.game.fens[Math.min(m.ply, res.game.fens.length - 1)];
        setFenCache((c) => new Map(c).set(selLine.hash, fen));
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  }, [selLine]);

  const asideFen = (selLine && fenCache.get(selLine.hash)) ?? null;

  /* ---- fingerprint table ---- */
  const fpCols: DataTableColumn<FingerprintRow>[] = [
    { key: "eco", header: "ECO", render: (r) => <span className="cell-eco">{r.eco}</span> },
    { key: "name", header: "OPENING", render: (r) => r.name ?? <span className="cell-dim">—</span> },
    {
      key: "share",
      header: "SHARE",
      align: "right",
      render: (r) => <span className="cell-mono">{r.sharePct.toFixed(0)}%</span>,
    },
    {
      key: "score",
      header: "SCORE",
      render: (r) => (
        <span className="prep2-score">
          <span className="prep2-score-bar">
            <BaselineBar fraction={r.scorePct / 100} tone={fingerprintRowWeak(r) ? "bad" : "good"} />
          </span>
          <span className={`prep2-score-num${fingerprintRowWeak(r) ? " bad" : ""}`}>
            {r.scorePct.toFixed(0)}%
          </span>
        </span>
      ),
    },
    {
      key: "elo",
      header: "AVG ELO",
      align: "right",
      render: () => (
        <span
          className="prep2-elo-none"
          title="Per-family average Elo is not recorded by the fingerprint yet."
        >
          —
        </span>
      ),
    },
    {
      key: "exit",
      header: "BOOK EXIT",
      render: (r) => {
        const exit = fp ? bookExitFor(r, fp.bookExits) : null;
        return exit ? (
          <span className="prep2-exit">{exit}</span>
        ) : (
          <span className="cell-dim">—</span>
        );
      },
    },
  ];

  /* ---- master games table ---- */
  const masters = selLine?.masterGames ?? [];
  const mgCols: DataTableColumn<MasterGame>[] = [
    {
      key: "white",
      header: "WHITE",
      render: (m) => (
        <>
          {m.white}
          {m.whiteElo ? <span className="cell-date"> {m.whiteElo}</span> : null}
        </>
      ),
    },
    {
      key: "black",
      header: "BLACK",
      render: (m) => (
        <>
          {m.black}
          {m.blackElo ? <span className="cell-date"> {m.blackElo}</span> : null}
        </>
      ),
    },
    { key: "result", header: "RESULT", render: (m) => <span className="cell-result">{m.result}</span> },
    { key: "event", header: "EVENT", render: (m) => <span className="cell-dim">{m.event}</span> },
    {
      key: "year",
      header: "YEAR",
      align: "right",
      render: (m) => <span className="cell-date">{m.date?.slice(0, 4) ?? "—"}</span>,
    },
    {
      key: "ply",
      header: "AT PLY",
      align: "right",
      render: (m) => <span className="cell-date">{m.ply}</span>,
    },
  ];

  const steps = stepperSteps(
    {
      opponent: selOpponent,
      color,
      lineName: selLine ? lineName(selLine) : null,
      masterCount: selLine ? selLine.masterGames.length : null,
    },
    reached,
  );

  const finding = prepFinding(selOpponent, profile);

  return (
    <div className="prep2">
      <ScreenHeader
        title="Opponent prep"
        subtitle="Pick an opponent, fingerprint the repertoire, find the soft lines"
      />
      <Stepper
        steps={steps}
        active={step - 1}
        onSelect={(i) => {
          const n = i + 1;
          if (n <= reached) setStep(n);
        }}
      />
      <div className="prep2-body">
        <div className="prep2-main">
          {step === 1 && (
            <div>
              <div className="prep2-search-row">
                <input
                  type="text"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && void search(query)}
                  placeholder="Opponent name…"
                  spellCheck={false}
                />
                <button className="btn-secondary" onClick={() => void search(query)} disabled={searching}>
                  {searching ? "Searching…" : "Search local"}
                </button>
                <button
                  className="btn-secondary"
                  disabled
                  title="Account fetch ships with the sync surface; today it runs from the CLI: kibitz-cli lichess-sync."
                >
                  Fetch from Lichess
                </button>
                <button
                  className="btn-secondary"
                  disabled
                  title="Account fetch ships with the sync surface; today it runs from the CLI: kibitz-cli chesscom-sync."
                >
                  Fetch from chess.com
                </button>
              </div>
              {error && <div className="error">{error}</div>}
              {candidates && candidates.length === 0 && (
                <div className="pf2-empty">No local players match “{query.trim()}”.</div>
              )}
              {candidates && candidates.length > 0 && (
                <div className="prep2-results">
                  {candidates.map((c, i) => (
                    <button
                      key={c.name}
                      type="button"
                      className={`prep2-result${(selOpponent ? selOpponent === c.name : i === 0) ? " sel" : ""}`}
                      onClick={() => selectOpponent(c.name)}
                    >
                      <span className="prep2-result-name">{c.name}</span>
                      <span className="prep2-result-games">
                        {c.games !== null ? `${c.games} game${c.games === 1 ? "" : "s"}` : ""}
                      </span>
                      <span className="source-tag dim">local</span>
                    </button>
                  ))}
                </div>
              )}
              <p className="prep2-footnote">
                Local hits come from your own database first. Fetching would pull that player&rsquo;s
                public games on demand — the engine stays cold; nothing is analysed until you ask
                for it. Account fetch currently runs from the CLI (kibitz-cli lichess-sync /
                chesscom-sync).
              </p>
            </div>
          )}

          {step === 2 && (
            <div>
              <div className="prep2-fp-head">
                <span className="prep2-strip-title">REPERTOIRE FINGERPRINT</span>
                <div className="seg" role="tablist" aria-label="Repertoire colour">
                  <button className={color === "white" ? "cur" : ""} onClick={() => setColor("white")}>
                    as White
                  </button>
                  <button className={color === "black" ? "cur" : ""} onClick={() => setColor("black")}>
                    as Black
                  </button>
                </div>
                {fp && (
                  <span className="prep2-fp-meta">
                    {fp.games} games as {color === "white" ? "White" : "Black"} · scores{" "}
                    {fp.scorePct.toFixed(1)}%
                  </span>
                )}
              </div>
              {error && <div className="error">{error}</div>}
              {loadingPrep && <div className="pf2-empty">Fingerprinting {selOpponent}…</div>}
              {fp && (
                <DataTable
                  columns={fpCols}
                  rows={fp.rows}
                  gridTemplate="64px 1.4fr 78px 1fr 84px 1fr"
                  rowKey={(r) => r.eco}
                  onRowClick={() => goTo(3)}
                  rowClassName={(r) => (fingerprintRowWeak(r) ? "prep2-weak-row" : undefined)}
                  empty="No games for this colour."
                />
              )}
              {fp && (
                <p className="prep2-footnote">
                  Weak families (under 50% with a real sample) are marked; click a row — or the
                  ③ chip — to rank the weak lines.
                </p>
              )}
            </div>
          )}

          {step === 3 && (
            <div>
              <div className="prep2-strip-title standalone">WEAKEST LINES — RANKED</div>
              {error && <div className="error">{error}</div>}
              {loadingPrep && <div className="pf2-empty">Ranking weak lines…</div>}
              {lines && lines.length === 0 && (
                <div className="pf2-empty">
                  No weak lines found for {selOpponent} as {color} (needs 3+ games per spot and an
                  under-50% score).
                </div>
              )}
              {lines && lines.length > 0 && (
                <div className="prep2-lines">
                  {lines.map((l, i) => (
                    <button
                      key={l.hash}
                      type="button"
                      className={`prep2-line${i === 0 ? " top" : ""}${selHash === l.hash ? " sel" : ""}`}
                      onClick={() => selectLine(l)}
                    >
                      <span className="prep2-line-head">
                        <span className="prep2-line-rank">{String(i + 1).padStart(2, "0")}</span>
                        <span className="prep2-line-name">{lineName(l)}</span>
                        <span className="prep2-line-moves">{lineMoves(l)}</span>
                        <span className="prep2-line-spacer" />
                        <span className="prep2-line-score">{lineScore(l)}</span>
                      </span>
                      <span className="prep2-line-why">{lineWhy(selOpponent ?? "", color, l)}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {step === 4 && (
            <div>
              <div className="prep2-strip-title standalone">MASTER GAMES IN THIS EXACT POSITION</div>
              {selLine && (
                <div className="prep2-line-context">
                  {lineName(selLine)} · {lineScore(selLine)} · {selOpponent} plays{" "}
                  {selLine.opponentMoves.join(" / ")}
                </div>
              )}
              <DataTable
                columns={mgCols}
                rows={masters}
                gridTemplate="1.4fr 1.4fr 58px 1fr 74px 96px"
                rowKey={(m) => m.gameId}
                onRowClick={(m) => onLoadGameAt(m.gameId, m.ply)}
                empty="No master games reach this spot."
              />
              <p className="prep2-footnote">{MASTER_RANKING_RULE}</p>
            </div>
          )}
        </div>

        <aside className="prep2-aside">
          <Board fen={asideFen ?? START_FEN} treatment={treatment} size={472} />
          <div className="prep2-aside-caption">
            {selLine && asideFen
              ? `${lineName(selLine).toUpperCase()} · BY PLY ${selLine.ply} · SCORES ${selLine.scorePct.toFixed(0)}%`
              : "NO PREP POSITION CHOSEN YET · STARTING POSITION"}
          </div>
          <div>
            <div className="prep2-finding-label">PROFILE FINDING · THIS OPPONENT</div>
            <p className="prep2-finding">{finding}</p>
            <div className="prep2-aside-buttons">
              <button
                className="btn-secondary"
                disabled={!selOpponent}
                onClick={() => selOpponent && onNavigate("profile", { opponent: selOpponent })}
              >
                Open his profile
              </button>
              <button
                className="btn-primary"
                disabled={!selLine || selLine.masterGames.length === 0}
                onClick={() => {
                  if (selLine && selLine.masterGames.length > 0) {
                    onLoadGameAt(selLine.masterGames[0].gameId, selLine.ply);
                  }
                }}
              >
                Study in game view
              </button>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}
