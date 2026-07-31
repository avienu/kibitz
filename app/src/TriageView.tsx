/**
 * Opening triage (run 10): after games sync in, show exactly where the
 * user's opening play needs work — ranked DEVIATIONS (left own book),
 * GAPS (opponent moves the book doesn't answer) and FRONTIERS (where the
 * book simply ends) per color — and grow the book where it ends with
 * engine-proposed lines (4-line deep MultiPV through the job queue;
 * adopting a line lands it in the repertoire as SRS cards).
 *
 * The report itself is a static database walk — the engine only runs
 * when the user clicks "Extend with engine" (CLAUDE.md #6).
 */
import { useCallback, useEffect, useState } from "react";
import Board from "./Board";
import ScreenHeader from "./shell/ScreenHeader";
import { matchingPlayers, selfPlayerGet, selfPlayerSet, trainAddLine } from "./lib/db";
import {
  evalLabel,
  itemCaption,
  numberedLine,
  triageExtend,
  triageExtensionStatus,
  triageReport,
  triageSummary,
  type ColorTriage,
  type ExtensionStatus,
  type TriageItem,
  type TriageReport,
} from "./lib/triage";
import type { BoardTreatment } from "./lib/evidence";

const PLAYER_KEY = "kibitz.triagePlayer";

export type TriageKind = "deviation" | "gap" | "frontier";

interface Selection {
  kind: TriageKind;
  item: TriageItem;
}

/** The three ranked lists for one color (pure — unit-testable). */
export function TriageLists({
  ct,
  selectedFen,
  onSelect,
}: {
  ct: ColorTriage;
  selectedFen: string | null;
  onSelect: (kind: TriageKind, item: TriageItem) => void;
}) {
  const section = (kind: TriageKind, title: string, items: TriageItem[]) => (
    <div className="triage-section" key={kind}>
      <div className="triage-strip-title">{title}</div>
      {items.length === 0 && <div className="triage-none">none found</div>}
      {items.map((it, i) => (
        <button
          key={it.fen}
          type="button"
          className={`triage-row${selectedFen === it.fen ? " sel" : ""}`}
          onClick={() => onSelect(kind, it)}
        >
          <span className="triage-rank">{String(i + 1).padStart(2, "0")}</span>
          <span className="triage-row-main">
            <span className="triage-line">{it.line || "start position"}</span>
            <span className="triage-caption">
              {itemCaption(kind, it)}
              {it.hasExtension ? " · engine lines ready" : ""}
            </span>
          </span>
          <span className="triage-count">{it.games}×</span>
        </button>
      ))}
    </div>
  );
  return (
    <>
      {section("deviation", "DEVIATIONS — YOU LEFT YOUR OWN BOOK", ct.deviations)}
      {section("gap", "GAPS — OPPONENT MOVES YOUR BOOK DOESN'T ANSWER", ct.gaps)}
      {section("frontier", "FRONTIERS — WHERE YOUR BOOK ENDS", ct.frontiers)}
    </>
  );
}

interface TriageViewProps {
  treatment?: BoardTreatment;
  /** Open a database game at a ply (deviations deep-link to the spot). */
  onOpenGameAt: (gameId: number, ply: number) => void;
  /** Adoption creates SRS cards — let the shell refresh its due badges. */
  onCountsChanged?: () => void;
}

export default function TriageView({
  treatment = "walnut",
  onOpenGameAt,
  onCountsChanged,
}: TriageViewProps) {
  const [player, setPlayer] = useState(() => localStorage.getItem(PLAYER_KEY) ?? "");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [report, setReport] = useState<TriageReport | null>(null);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [color, setColor] = useState<"white" | "black">("white");
  const [sel, setSel] = useState<Selection | null>(null);

  // The app knows who you are (2026-07-30): empty field seeds from the
  // database's canonical self_player (localStorage is only the
  // screen-local last-used and dies with webview storage).
  useEffect(() => {
    if (player.trim() !== "") return;
    selfPlayerGet()
      .then((name) => {
        if (name) setPlayer((cur) => (cur.trim() === "" ? name : cur));
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [extStatus, setExtStatus] = useState<ExtensionStatus | null>(null);
  const [extError, setExtError] = useState<string | null>(null);
  const [adoptMsg, setAdoptMsg] = useState<string | null>(null);
  const [adopting, setAdopting] = useState(false);

  /* ---- player suggestions (debounced) ---- */
  useEffect(() => {
    const q = player.trim();
    if (q.length < 2) return;
    const t = setTimeout(() => {
      matchingPlayers(q)
        .then((names) => setSuggestions(names.slice(0, 8)))
        .catch(() => setSuggestions([]));
    }, 250);
    return () => clearTimeout(t);
  }, [player]);

  /* ---- build the report ---- */
  const run = useCallback(async () => {
    const p = player.trim();
    if (p === "" || building) return;
    setBuilding(true);
    setError(null);
    setSel(null);
    try {
      const r = await triageReport(p);
      setReport(r);
      localStorage.setItem(PLAYER_KEY, p);
      selfPlayerSet(p).catch(() => {}); // running triage declares self
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setBuilding(false);
    }
  }, [player, building]);

  const ct: ColorTriage | null = report ? (color === "white" ? report.white : report.black) : null;

  const select = useCallback((kind: TriageKind, item: TriageItem) => {
    setSel({ kind, item });
    setExtStatus(null);
    setExtError(null);
    setAdoptMsg(null);
  }, []);

  /* ---- extension status: fetch on selection, poll while queued/running ---- */
  const selFen = sel && sel.kind !== "deviation" ? sel.item.fen : null;
  const pollBusy = extStatus?.jobStatus === "pending" || extStatus?.jobStatus === "running";
  useEffect(() => {
    if (!selFen) return;
    let stale = false;
    const fetchStatus = () => {
      triageExtensionStatus(selFen)
        .then((s) => {
          if (!stale) setExtStatus(s);
        })
        .catch((e) => {
          if (!stale) setExtError(String(e));
        });
    };
    fetchStatus();
    if (!pollBusy) return () => {
      stale = true;
    };
    const t = setInterval(fetchStatus, 2500);
    return () => {
      stale = true;
      clearInterval(t);
    };
  }, [selFen, pollBusy]);

  // Once a result lands, reflect it in the list badge without a rebuild.
  useEffect(() => {
    if (!extStatus?.extension || !sel || sel.item.hasExtension) return;
    const fen = sel.item.fen;
    setReport((r) => {
      if (!r) return r;
      const mark = (c: ColorTriage): ColorTriage => ({
        ...c,
        gaps: c.gaps.map((i) => (i.fen === fen ? { ...i, hasExtension: true } : i)),
        frontiers: c.frontiers.map((i) => (i.fen === fen ? { ...i, hasExtension: true } : i)),
      });
      return { ...r, white: mark(r.white), black: mark(r.black) };
    });
    setSel((s) => (s ? { ...s, item: { ...s.item, hasExtension: true } } : s));
  }, [extStatus, sel]);

  const extend = useCallback(async () => {
    if (!selFen) return;
    setExtError(null);
    try {
      await triageExtend(selFen);
      // Optimistic queued state; the poller takes over from here.
      setExtStatus((s) => ({
        extension: s?.extension ?? null,
        jobStatus: "pending",
        jobsAhead: s?.jobsAhead ?? 0,
        workerActive: true,
      }));
    } catch (e) {
      setExtError(String(e));
    }
  }, [selFen]);

  const adopt = useCallback(
    async (sans: string[]) => {
      if (!sel || adopting) return;
      setAdopting(true);
      setAdoptMsg(null);
      try {
        const res = await trainAddLine(color, sans, sel.item.fen);
        setAdoptMsg(
          `Adopted into "${res.repertoire}": ${res.cardsAdded} new card${
            res.cardsAdded === 1 ? "" : "s"
          }, ${res.cardsExisting} position${res.cardsExisting === 1 ? "" : "s"} already covered.`,
        );
        onCountsChanged?.();
      } catch (e) {
        setAdoptMsg(`Adoption failed: ${e}`);
      } finally {
        setAdopting(false);
      }
    },
    [sel, color, adopting, onCountsChanged],
  );

  const extension = extStatus?.extension ?? null;

  return (
    <div className="triage2">
      <ScreenHeader
        title="Opening triage"
        subtitle="Where your games left your book — and where the book should grow"
        actions={
          <div className="seg" role="tablist" aria-label="Repertoire colour">
            <button
              className={color === "white" ? "cur" : ""}
              onClick={() => {
                setColor("white");
                setSel(null);
              }}
            >
              as White
            </button>
            <button
              className={color === "black" ? "cur" : ""}
              onClick={() => {
                setColor("black");
                setSel(null);
              }}
            >
              as Black
            </button>
          </div>
        }
      />
      <div className="triage-body">
        <div className="triage-main">
          <div className="triage-search-row">
            <input
              type="text"
              value={player}
              onChange={(e) => setPlayer(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void run()}
              placeholder="Your name as it appears in your games…"
              list="triage-player-suggestions"
              spellCheck={false}
            />
            <datalist id="triage-player-suggestions">
              {suggestions.map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
            <button
              className="btn-primary"
              onClick={() => void run()}
              disabled={building || player.trim() === ""}
            >
              {building ? "Walking your games…" : "Run triage"}
            </button>
          </div>
          {error && <div className="error">{error}</div>}
          {!report && !error && (
            <p className="triage-footnote">
              Walks your recent games (all your name forms and declared aliases count as you)
              against your repertoire cards, both colors. Static database work — the engine
              stays off until you explicitly ask for an extension.
            </p>
          )}
          {report && ct && (
            <>
              <div className="triage-summary">
                {report.player} as {color === "white" ? "White" : "Black"} ·{" "}
                {ct.gamesScanned} game{ct.gamesScanned === 1 ? "" : "s"} scanned ·{" "}
                {triageSummary(ct)}
              </div>
              {!ct.hasCards ? (
                <div className="pf2-empty">
                  No {color} repertoire cards yet — add lines from the Game view (&ldquo;→
                  repertoire&rdquo;) or import a PGN study, then re-run the triage.
                </div>
              ) : (
                <TriageLists ct={ct} selectedFen={sel?.item.fen ?? null} onSelect={select} />
              )}
            </>
          )}
        </div>

        <aside className="triage-aside">
          <Board
            fen={sel?.item.fen ?? "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}
            orientation={color}
            treatment={treatment}
            size={360}
          />
          {!sel && <div className="triage-aside-caption">SELECT A ROW TO SEE THE POSITION</div>}
          {sel && (
            <>
              <div className="triage-aside-caption">
                {[
                  sel.item.eco && sel.item.openingName
                    ? `${sel.item.eco} ${sel.item.openingName}`
                    : null,
                  `PLY ${sel.item.ply}`,
                  `${sel.item.games} GAME${sel.item.games === 1 ? "" : "S"}`,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </div>
              <div className="triage-detail">
                <div className="triage-detail-line">{sel.item.line || "start position"}</div>
                <div className="triage-detail-caption">{itemCaption(sel.kind, sel.item)}</div>
              </div>

              <div className="triage-strip-title">SOURCE GAMES</div>
              <div className="triage-examples">
                {sel.item.examples.map((ex) => (
                  <button
                    key={`${ex.gameId}-${ex.ply}`}
                    type="button"
                    className="triage-example"
                    onClick={() => onOpenGameAt(ex.gameId, ex.ply)}
                  >
                    #{ex.gameId} {ex.white} — {ex.black}
                    {ex.date ? ` · ${ex.date}` : ""}
                    {ex.playedSan ? ` · played ${ex.playedSan}` : ""} · ply {ex.ply}
                  </button>
                ))}
              </div>

              {sel.kind !== "deviation" && (
                <div className="triage-extend">
                  <div className="triage-strip-title">EXTEND THE BOOK</div>
                  {extError && <div className="error">{extError}</div>}
                  {extension ? (
                    <>
                      <div className="triage-ext-meta">
                        {extension.engine} · depth {extension.depth} · {extension.lines.length}{" "}
                        line{extension.lines.length === 1 ? "" : "s"}
                      </div>
                      {extension.lines.map((line, i) => (
                        <div className="triage-ext-line" key={i}>
                          <span className="triage-ext-eval">{evalLabel(line, extension.fen)}</span>
                          <span className="triage-ext-sans">
                            {numberedLine(line.sans, extension.fen)}
                          </span>
                          <button
                            className="btn-secondary"
                            disabled={adopting}
                            onClick={() => void adopt(line.sans)}
                          >
                            Adopt
                          </button>
                        </div>
                      ))}
                      {adoptMsg && <div className="triage-adopt-msg">{adoptMsg}</div>}
                    </>
                  ) : extStatus?.jobStatus === "pending" ? (
                    <div className="triage-ext-progress">
                      Queued for the engine
                      {extStatus.jobsAhead > 0
                        ? ` — ${extStatus.jobsAhead} job${
                            extStatus.jobsAhead === 1 ? "" : "s"
                          } ahead of it`
                        : ""}
                      {extStatus.workerActive ? " · worker running…" : " · worker starting…"}
                    </div>
                  ) : extStatus?.jobStatus === "running" ? (
                    <div className="triage-ext-progress">
                      Engine analysing — 4 lines, deep search. This can take a few minutes…
                    </div>
                  ) : (
                    <>
                      {extStatus?.jobStatus === "failed" && (
                        <div className="error">The last extension attempt failed — retry below.</div>
                      )}
                      <button className="btn-primary" onClick={() => void extend()}>
                        Extend with engine (4 lines)
                      </button>
                      <p className="triage-footnote">
                        Deep MultiPV analysis (depth 30) through the job queue. Clicking is the
                        explicit engine request: the job is queued and the worker starts now.
                      </p>
                    </>
                  )}
                </div>
              )}
            </>
          )}
        </aside>
      </div>
    </div>
  );
}
