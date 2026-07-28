/**
 * Play online (run 10): live games on lichess via the Board API. Seek →
 * play on the standard Board → the finished game auto-imports with
 * provenance and shows up on Home like any other personal/online game.
 *
 * FAIR PLAY IS STRUCTURAL (lichess ToS; a product principle like
 * engine-off): this screen mounts NO engine, explain, eval or suggestion
 * surface — there is nothing to reach for while a game runs — and says
 * so visibly. PlayView.test.tsx gates this at the import level.
 *
 * Honesty rules: only the time controls lichess allows third-party
 * clients (rapid, classical, correspondence) are offered; the seek card
 * says why there is no bullet/blitz. Clocks tick locally from the last
 * server state and resync on every stream message.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Board, { type BoardMovable } from "./Board";
import ScreenHeader from "./shell/ScreenHeader";
import { usePromotionPicker } from "./PromotionPicker";
import { PROMO_UCI, type PromoRole } from "./lib/promotion";
import type { BoardTreatmentChoice } from "./lib/gameView";
import type { ViewId } from "./lib/shell";
import {
  clocksAt,
  estimatedSpeed,
  FAIR_PLAY_NOTICE,
  fmtClock,
  isTerminal,
  legalDests,
  lichessTokenStatus,
  nowPlaying,
  numberedSans,
  onPlayEvent,
  onPlayGame,
  onPlaySeek,
  playAbort,
  playDraw,
  playJoin,
  playMove,
  playResign,
  playSeek,
  playStart,
  resultLine,
  seekCancel,
  stepsFromUci,
  turnOf,
  type GameSnapshot,
  type LichessTokenStatus,
  type NowPlaying,
} from "./lib/lichessPlay";

interface PlayViewProps {
  treatment: BoardTreatmentChoice;
  onNavigate: (id: ViewId) => void;
}

/** Board-API-legal realtime presets (estimatedSpeed labels each honestly). */
const PRESETS: { minutes: number; increment: number }[] = [
  { minutes: 10, increment: 0 },
  { minutes: 10, increment: 5 },
  { minutes: 15, increment: 10 },
  { minutes: 30, increment: 0 },
  { minutes: 30, increment: 20 },
];

const CORR_DAYS = [1, 2, 3, 5, 7, 14];

interface ActiveGame {
  snap: GameSnapshot;
  /** Date.now() when the snapshot arrived (local clock tick base). */
  at: number;
}

export default function PlayView({ treatment, onNavigate }: PlayViewProps) {
  const [token, setToken] = useState<LichessTokenStatus | null>(null);
  const [games, setGames] = useState<NowPlaying[]>([]);
  const [active, setActive] = useState<ActiveGame | null>(null);
  const [seeking, setSeeking] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [confirmResign, setConfirmResign] = useState(false);

  // Seek form.
  const [preset, setPreset] = useState(0);
  const [corr, setCorr] = useState(false);
  const [days, setDays] = useState(3);
  const [rated, setRated] = useState(false);
  const [color, setColor] = useState("random");

  /** The game the user is looking at; stream events for others are
   * ignored (their board threads still run and still import). */
  const activeIdRef = useRef<string | null>(null);
  const seekingRef = useRef(false);
  seekingRef.current = seeking;

  const refreshGames = useCallback(() => {
    nowPlaying()
      .then(setGames)
      .catch(() => setGames([]));
  }, []);

  const join = useCallback((gameId: string) => {
    activeIdRef.current = gameId;
    setConfirmResign(false);
    playJoin(gameId)
      .then((snap) => {
        if (snap && activeIdRef.current === gameId) {
          setActive({ snap, at: Date.now() });
        }
      })
      .catch((e) => setNote(String(e)));
  }, []);

  /* ---- boot: token status, event stream, ongoing games ---- */
  useEffect(() => {
    let cancelled = false;
    lichessTokenStatus()
      .then((t) => {
        if (cancelled) return;
        setToken(t);
        if (t.configured) {
          playStart().catch((e) => setNote(String(e)));
          refreshGames();
        }
      })
      .catch((e) => !cancelled && setNote(String(e)));
    return () => {
      cancelled = true;
    };
  }, [refreshGames]);

  /* ---- stream events ---- */
  useEffect(() => {
    const unsubs: Array<() => void> = [];
    onPlayGame((snap) => {
      if (snap.gameId === activeIdRef.current) setActive({ snap, at: Date.now() });
    })
      .then((u) => unsubs.push(u))
      .catch(() => {}); // vite-only dev: no Tauri runtime
    onPlayEvent((ev) => {
      if (ev.kind === "gameStart") {
        refreshGames();
        // A fulfilled seek flows straight onto the board.
        if (seekingRef.current || activeIdRef.current === null) join(ev.gameId);
      } else if (ev.kind === "gameFinish") {
        refreshGames();
      } else if (ev.kind === "imported") {
        setNote(`Game ${ev.gameId} imported — it is on Home under “New since”.`);
      } else if (ev.kind === "error" && ev.detail) {
        setNote(ev.detail);
      }
    })
      .then((u) => unsubs.push(u))
      .catch(() => {});
    onPlaySeek((ev) => {
      setSeeking(ev.active);
      if (ev.error) setNote(ev.error);
    })
      .then((u) => unsubs.push(u))
      .catch(() => {});
    return () => unsubs.forEach((u) => u());
  }, [join, refreshGames]);

  /* ---- local clock tick (0.5 s) while a live game is shown ---- */
  const liveGame = active !== null && !isTerminal(active.snap.status);
  useEffect(() => {
    if (!liveGame) return;
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, [liveGame]);

  /* ---- board model ---- */
  const snap = active?.snap ?? null;
  const steps = useMemo(
    () => (snap ? stepsFromUci(snap.initialFen, snap.moves) : null),
    [snap],
  );
  const fen = steps ? steps.fens[steps.fens.length - 1] : (snap?.initialFen ?? null);
  const lastMove = useMemo((): [string, string] | undefined => {
    const last = snap?.moves[snap.moves.length - 1];
    return last && last.length >= 4 ? [last.slice(0, 2), last.slice(2, 4)] : undefined;
  }, [snap]);

  const myTurn = snap !== null && !isTerminal(snap.status) && snap.myColor === turnOf(snap);

  const boardMoveRef = useRef<(orig: string, dest: string, role?: PromoRole) => void>(() => {});
  const promo = usePromotionPicker((orig, dest, role) => boardMoveRef.current(orig, dest, role));

  const handleMove = useCallback(
    (orig: string, dest: string, promoRole?: PromoRole) => {
      if (!snap || !fen) return;
      if (!promoRole && promo.guard(fen, orig, dest)) return;
      const uci = orig + dest + (promoRole ? PROMO_UCI[promoRole] : "");
      playMove(snap.gameId, uci).catch((e) => setNote(String(e)));
    },
    [snap, fen, promo],
  );
  boardMoveRef.current = handleMove;

  const movable = useMemo((): BoardMovable | undefined => {
    if (!myTurn || !snap || !fen) return undefined;
    const dests = legalDests(fen);
    if (!dests) return undefined;
    return {
      color: snap.myColor as "white" | "black",
      dests,
      onMove: handleMove,
    };
  }, [myTurn, snap, fen, handleMove]);

  /* ---- actions ---- */
  const doSeek = useCallback(() => {
    setNote(null);
    const opts = corr
      ? { days, rated, color }
      : { minutes: PRESETS[preset].minutes, increment: PRESETS[preset].increment, rated, color };
    playSeek(opts)
      .then(() => {
        if (corr) {
          setNote("Correspondence seek created on lichess — the game appears here when someone joins.");
          refreshGames();
        }
      })
      .catch((e) => setNote(String(e)));
  }, [corr, days, preset, rated, color, refreshGames]);

  const act = useCallback((p: Promise<void>) => {
    p.catch((e) => setNote(String(e)));
  }, []);

  /* ---- render ---- */

  if (token === null) {
    return (
      <>
        <ScreenHeader title="Play online" subtitle="lichess Board API" />
        <div className="page-scroll">
          <div className="play-page">Checking the lichess token…</div>
        </div>
      </>
    );
  }

  if (!token.configured) {
    return (
      <>
        <ScreenHeader title="Play online" subtitle="lichess Board API" />
        <div className="page-scroll">
          <div className="play-page">
            <div className="sync-card">
              <div className="sync-card-head">CONNECT A LICHESS ACCOUNT</div>
              <p className="sync-blurb">
                Playing needs a lichess personal access token with the <b>board:play</b> scope.
                Create one at lichess.org → Preferences → API access tokens, then paste it in
                Settings. It is stored on this machine only (owner-readable file), never in the
                database, and never shown again in full.
              </p>
              <div className="sync-row">
                <button className="btn-primary" onClick={() => onNavigate("settings")}>
                  Open Settings
                </button>
              </div>
            </div>
            <div className="play-fairplay">{FAIR_PLAY_NOTICE}</div>
          </div>
        </div>
      </>
    );
  }

  const clocks = snap ? clocksAt(snap, active?.at ?? now, now) : null;
  const finished = snap ? resultLine(snap) : null;
  const opponentOffersDraw =
    snap !== null && (snap.myColor === "white" ? snap.bdraw : snap.myColor === "black" ? snap.wdraw : false);
  const canAbort = snap !== null && !isTerminal(snap.status) && snap.moves.length < 2;

  const playerRow = (side: "white" | "black") => {
    if (!snap || !clocks) return null;
    const name = side === "white" ? snap.white : snap.black;
    const rating = side === "white" ? snap.whiteRating : snap.blackRating;
    const ms = side === "white" ? clocks.whiteMs : clocks.blackMs;
    const toMove = !isTerminal(snap.status) && turnOf(snap) === side;
    return (
      <div className={`play-player${toMove ? " to-move" : ""}`}>
        <span className="play-player-name">
          {name}
          {rating !== null ? ` (${rating})` : ""}
          {snap.myColor === side ? " — you" : ""}
        </span>
        <span className="play-clock">{fmtClock(ms)}</span>
      </div>
    );
  };

  const orientation = (snap?.myColor as "white" | "black" | null) ?? "white";
  const topSide = orientation === "white" ? "black" : "white";

  return (
    <>
      <ScreenHeader
        title="Play online"
        subtitle={`lichess Board API · signed in as ${token.username ?? "?"}`}
      />
      <div className="page-scroll">
        <div className="play-page">
          <div className="play-fairplay">{FAIR_PLAY_NOTICE}</div>
          {note && <div className="play-note">{note}</div>}
          <div className="play-columns">
            <div className="play-board-col">
              {snap && fen ? (
                <>
                  {playerRow(topSide)}
                  <div style={{ position: "relative" }}>
                    <Board
                      fen={fen}
                      lastMove={lastMove}
                      movable={movable}
                      orientation={orientation}
                      treatment={treatment}
                    />
                    {promo.element}
                  </div>
                  {playerRow(orientation)}
                  {finished ? (
                    <div className="play-result">
                      {finished} · imported for review — analyze it from the Game view whenever
                      you like.
                    </div>
                  ) : (
                    <div className="play-actions">
                      {canAbort && (
                        <button className="btn-ghost" onClick={() => act(playAbort(snap.gameId))}>
                          Abort
                        </button>
                      )}
                      {opponentOffersDraw ? (
                        <>
                          <button
                            className="btn-secondary"
                            onClick={() => act(playDraw(snap.gameId, true))}
                          >
                            Accept draw
                          </button>
                          <button
                            className="btn-ghost"
                            onClick={() => act(playDraw(snap.gameId, false))}
                          >
                            Decline draw
                          </button>
                        </>
                      ) : (
                        <button
                          className="btn-ghost"
                          onClick={() => act(playDraw(snap.gameId, true))}
                        >
                          Offer draw
                        </button>
                      )}
                      {confirmResign ? (
                        <>
                          <button
                            className="btn-secondary"
                            onClick={() => {
                              setConfirmResign(false);
                              act(playResign(snap.gameId));
                            }}
                          >
                            Confirm resign
                          </button>
                          <button className="btn-ghost" onClick={() => setConfirmResign(false)}>
                            Keep playing
                          </button>
                        </>
                      ) : (
                        <button className="btn-ghost" onClick={() => setConfirmResign(true)}>
                          Resign
                        </button>
                      )}
                    </div>
                  )}
                  {steps && steps.sans.length > 0 && (
                    <div className="play-moves mono">{numberedSans(steps.sans)}</div>
                  )}
                </>
              ) : (
                <div className="play-empty">
                  No game on the board — seek one, or rejoin an ongoing game.
                </div>
              )}
            </div>

            <div className="play-side-col">
              <div className="sync-card">
                <div className="sync-card-head">SEEK A GAME</div>
                <p className="sync-blurb">
                  Lichess allows third-party clients rapid, classical and correspondence only —
                  no bullet or blitz.
                </p>
                <div className="play-seek-modes">
                  <label>
                    <input type="radio" checked={!corr} onChange={() => setCorr(false)} /> Realtime
                  </label>
                  <label>
                    <input type="radio" checked={corr} onChange={() => setCorr(true)} />{" "}
                    Correspondence
                  </label>
                </div>
                {corr ? (
                  <div className="sync-row">
                    <label>
                      Days per move{" "}
                      <select value={days} onChange={(e) => setDays(Number(e.target.value))}>
                        {CORR_DAYS.map((d) => (
                          <option key={d} value={d}>
                            {d}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                ) : (
                  <div className="play-presets">
                    {PRESETS.map((p, i) => (
                      <button
                        key={`${p.minutes}+${p.increment}`}
                        className={`btn-ghost${i === preset ? " active" : ""}`}
                        onClick={() => setPreset(i)}
                      >
                        {p.minutes}+{p.increment}
                        <span className="play-preset-speed">
                          {estimatedSpeed(p.minutes, p.increment) ?? "—"}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
                <div className="sync-row">
                  <label>
                    <input
                      type="checkbox"
                      checked={rated}
                      onChange={(e) => setRated(e.target.checked)}
                    />{" "}
                    Rated
                  </label>
                  <label>
                    Color{" "}
                    <select value={color} onChange={(e) => setColor(e.target.value)}>
                      <option value="random">random</option>
                      <option value="white">white</option>
                      <option value="black">black</option>
                    </select>
                  </label>
                </div>
                <div className="sync-row">
                  {seeking ? (
                    <>
                      <span className="sync-running">
                        <span className="strip-dot on" /> Searching for an opponent…
                      </span>
                      <button
                        className="btn-secondary"
                        onClick={() => seekCancel().catch(() => {})}
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <button className="btn-primary" onClick={doSeek}>
                      Seek
                    </button>
                  )}
                </div>
              </div>

              <div className="sync-card">
                <div className="sync-card-head">ONGOING GAMES</div>
                {games.length === 0 ? (
                  <p className="sync-blurb">
                    None right now. Correspondence games survive app restarts and reappear here.
                  </p>
                ) : (
                  games.map((g) => (
                    <div key={g.gameId} className="play-ongoing-row">
                      <span>
                        vs {g.opponent} · {g.speed}
                        {g.isMyTurn ? " · your move" : ""}
                      </span>
                      <button className="btn-secondary" onClick={() => join(g.gameId)}>
                        Rejoin
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
