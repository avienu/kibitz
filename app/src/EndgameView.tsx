/**
 * Endgames tab (ROADMAP Phase 5): a tiered curriculum of classic
 * theoretical positions (tier list → drill list → play view). The user
 * plays the goal side on this tab's own board; the opponent replies come
 * from the Rust side — Syzygy tablebase where the piece count is covered,
 * a documented deterministic heuristic otherwise. No engine anywhere in
 * this flow (CLAUDE.md #6).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Board, { type BoardMovable } from "./Board";
import {
  destsFor,
  endgameGiveUp,
  endgameMove,
  endgameOverview,
  endgameStart,
  goalText,
  lastMoveOf,
  masteryLabel,
  uciForDrag,
  type DrillInfo,
  type DrillProgress,
  type Outcome,
  type Overview,
  type StartedDrill,
} from "./lib/endgame";

type Phase = "playing" | "solved" | "failed";

interface PlayState {
  started: StartedDrill;
  fen: string;
  lastMove?: [string, string];
  phase: Phase;
  outcomeText: string;
}

export default function EndgameView() {
  const [ov, setOv] = useState<Overview | null>(null);
  const [status, setStatus] = useState("Pick a tier, then a drill.");
  const [tierId, setTierId] = useState<string | null>(null);
  const [play, setPlay] = useState<PlayState | null>(null);
  const busyRef = useRef(false); // one move in flight at a time
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadOverview = useCallback(() => {
    endgameOverview()
      .then(setOv)
      .catch((e) => setStatus(`Endgames unavailable (open a database first): ${e}`));
  }, []);
  useEffect(loadOverview, [loadOverview]);
  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    },
    [],
  );

  const finish = useCallback(
    (outcome: Outcome, progress: DrillProgress | null) => {
      setPlay((p) => {
        if (!p) return p;
        let note = "";
        if (progress) {
          note = progress.mastered
            ? " Drill mastered."
            : progress.cleanStreak > 0
              ? ` Clean streak ${progress.cleanStreak}/${ov?.masteryStreak ?? 2}.`
              : "";
        }
        return {
          ...p,
          phase: outcome.solved ? "solved" : "failed",
          outcomeText: `${outcome.detail}${note}`,
        };
      });
      loadOverview();
    },
    [ov, loadOverview],
  );

  const start = useCallback(async (drillId: string) => {
    try {
      if (timerRef.current) clearTimeout(timerRef.current);
      const started = await endgameStart(drillId);
      setPlay({ started, fen: started.fen, phase: "playing", outcomeText: "" });
      setStatus(
        started.opponentTablebase
          ? "Opponent: tablebase (optimal replies)."
          : "Opponent: heuristic sparring partner.",
      );
    } catch (e) {
      setStatus(String(e));
    }
  }, []);

  const onBoardMove = useCallback(
    (orig: string, dest: string) => {
      if (!play || play.phase !== "playing" || busyRef.current) return;
      const uci = uciForDrag(play.fen, orig, dest);
      if (!uci) return;
      busyRef.current = true;
      endgameMove(uci)
        .then((r) => {
          setPlay((p) => (p ? { ...p, fen: r.fenAfterUser, lastMove: lastMoveOf(uci) } : p));
          const opp = r.opponent;
          if (opp && r.fenAfterOpponent) {
            const oppFen = r.fenAfterOpponent;
            timerRef.current = setTimeout(() => {
              setPlay((p) => (p ? { ...p, fen: oppFen, lastMove: lastMoveOf(opp.uci) } : p));
              if (r.outcome) finish(r.outcome, r.progress);
            }, 350);
          } else if (r.outcome) {
            finish(r.outcome, r.progress);
          }
        })
        .catch((e) => setStatus(String(e)))
        .finally(() => {
          busyRef.current = false;
        });
    },
    [play, finish],
  );

  const giveUp = useCallback(async () => {
    try {
      const r = await endgameGiveUp();
      finish({ solved: false, detail: "Gave up." }, r.progress);
    } catch (e) {
      setStatus(String(e));
    }
  }, [finish]);

  const movable = useMemo((): BoardMovable | undefined => {
    if (!play || play.phase !== "playing") return undefined;
    const dests = destsFor(play.fen, play.started.userSide);
    if (dests.size === 0) return undefined; // opponent's turn (reply pending)
    return { color: play.started.userSide, dests, onMove: onBoardMove };
  }, [play, onBoardMove]);

  const drillsOfTier = useCallback(
    (tier: string): DrillInfo[] => (ov ? ov.drills.filter((d) => d.tier === tier) : []),
    [ov],
  );
  const masteredOf = useCallback(
    (tier: string): number => drillsOfTier(tier).filter((d) => d.mastered).length,
    [drillsOfTier],
  );

  const tier = tierId && ov ? ov.tiers.find((t) => t.id === tierId) : undefined;
  const masteredTotal = ov ? ov.drills.filter((d) => d.mastered).length : 0;

  return (
    <div className="tactics">
      <div className="db-summary">
        {ov
          ? `Endgame curriculum: ${ov.drills.length} drills, ${masteredTotal} mastered ` +
            `(${ov.masteryStreak} clean completions each).`
          : "Open a database (Database tab) to train endgames."}
      </div>
      {ov && <div className="tactics-hint">{ov.tablebase.note}</div>}

      {play && (
        <div className="db-section">
          <h3>{play.started.title}</h3>
          <div className="tactics-line">
            <span className={`tactics-phase ${play.phase === "playing" ? "solving" : play.phase}`}>
              {play.phase === "playing" && goalText(play.started.goal, play.started.userSide)}
              {play.phase === "solved" && "Solved ✓"}
              {play.phase === "failed" && "Failed ✗"}
            </span>
            <span className="tactics-meta">{play.started.drillId}</span>
          </div>
          <div className="tactics-hint">{play.started.instruction}</div>
          <div className="tactics-board">
            <Board
              fen={play.fen}
              lastMove={play.lastMove}
              movable={movable}
              orientation={play.started.userSide}
            />
          </div>
          {play.outcomeText && <div className="tactics-outcome">{play.outcomeText}</div>}
          <div className="engine-row">
            {play.phase === "playing" ? (
              <button onClick={() => void giveUp()}>Give up</button>
            ) : (
              <button onClick={() => void start(play.started.drillId)}>Retry</button>
            )}
            <button
              onClick={() => {
                if (play.phase === "playing") void giveUp();
                setPlay(null);
              }}
            >
              Back to drills
            </button>
          </div>
        </div>
      )}

      {!play && ov && !tier && (
        <div className="db-section">
          <h3>Tiers</h3>
          <table className="tactics-table">
            <thead>
              <tr>
                <th>tier</th>
                <th>rating band</th>
                <th>mastered</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {ov.tiers.map((t) => (
                <tr key={t.id}>
                  <td title={t.summary}>{t.name}</td>
                  <td>{t.ratingBand}</td>
                  <td>
                    {masteredOf(t.id)}/{drillsOfTier(t.id).length}
                  </td>
                  <td>
                    <button onClick={() => setTierId(t.id)}>Open</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {!play && ov && tier && (
        <div className="db-section">
          <h3>
            {tier.name} <span className="tactics-meta">({tier.ratingBand})</span>
          </h3>
          <div className="tactics-hint">{tier.summary}</div>
          <table className="tactics-table">
            <thead>
              <tr>
                <th>drill</th>
                <th>material</th>
                <th>goal</th>
                <th>tries</th>
                <th>mastery</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {drillsOfTier(tier.id).map((d) => (
                <tr key={d.id}>
                  <td title={d.instruction}>
                    {d.mastered ? "✓ " : ""}
                    {d.title}
                  </td>
                  <td>{d.material}</td>
                  <td>{d.goal}</td>
                  <td>{d.attempts}</td>
                  <td>{masteryLabel(d.cleanStreak, ov.masteryStreak, d.mastered)}</td>
                  <td>
                    <button onClick={() => void start(d.id)}>Start</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="engine-row">
            <button onClick={() => setTierId(null)}>All tiers</button>
          </div>
        </div>
      )}

      <div className="status">{status}</div>
    </div>
  );
}
