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
import { useCallback, useEffect, useRef, useState } from "react";
import Board, { type BoardMovable } from "./Board";
import ScrubLine, { type ScrubPreview } from "./components/ScrubLine";
import ScreenHeader from "./shell/ScreenHeader";
import { usePromotionPicker } from "./PromotionPicker";
import { identityGroup, selfPlayerGet, trainAddLine } from "./lib/db";
import { sanForBoardMove, trainDests } from "./lib/train";
import {
  answerConfirmCopy,
  answerLineSans,
  colorName,
  defaultTriageColor,
  evalLabel,
  inBookGaps,
  continuationDepths,
  inferredLineLabel,
  itemCaption,
  lineSans,
  realityDeviations,
  realityHeadline,
  triageExtend,
  searchProgressFraction,
  searchProgressLabel,
  type LiveSearch,
  triageExtensionStatus,
  triageInferFrom,
  triageInferRepertoire,
  triageReport,
  triageSummary,
  wholeGapLabel,
  wholeOpeningGaps,
  type ColorTriage,
  type ExtensionStatus,
  type InferredLine,
  type InferredRepertoire,
  type TriageItem,
  type TriageReport,
} from "./lib/triage";
import type { BoardTreatment } from "./lib/evidence";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

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

/** One inferred line: rank, scrubbable SAN, caption, Adopt. Lines that
 * continue an earlier one sit indented under it with the shared opening
 * moves dimmed — the list is a trunk plus its detail, not a wall of
 * near-identical lines. */
function InferLineRow({
  line,
  rank,
  shared,
  adopting,
  onPreview,
  onAdopt,
}: {
  line: InferredLine;
  rank: number;
  shared: number;
  adopting: boolean;
  onPreview: (p: ScrubPreview | null) => void;
  onAdopt: () => void;
}) {
  return (
    <div className={`triage-infer-line${shared > 0 ? " triage-infer-cont" : ""}`}>
      <span className="triage-rank">{String(rank).padStart(2, "0")}</span>
      <span className="triage-row-main">
        <ScrubLine
          className="triage-line"
          sans={line.sans}
          dimBefore={shared}
          onPreview={onPreview}
        />
        <span className="triage-caption">{inferredLineLabel(line)}</span>
      </span>
      <button className="btn-secondary" disabled={adopting} onClick={onAdopt}>
        Adopt
      </button>
    </div>
  );
}

/** The engine working, not a spinner: how deep it has got, and the lines
 * it likes at that depth. They reorder and change as the search deepens —
 * that IS the sausage being made, so it is labelled as provisional. */
function LiveSearchPanel({
  search,
  onPreview,
}: {
  search: LiveSearch;
  onPreview: (p: ScrubPreview | null) => void;
}) {
  return (
    <>
      <div
        className="triage-ext-bar"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={search.targetDepth}
        aria-valuenow={search.depth}
        aria-label="Search depth reached"
      >
        <span style={{ width: `${searchProgressFraction(search) * 100}%` }} />
      </div>
      {search.lines.map((line, i) => (
        <div className="triage-ext-line triage-ext-live" key={i}>
          <span className="triage-ext-eval">{evalLabel(line, search.fen)}</span>
          <ScrubLine
            className="triage-ext-sans"
            sans={line.sans}
            startFen={search.fen}
            onPreview={onPreview}
          />
        </div>
      ))}
      <p className="triage-footnote">
        Still searching: these are the engine's picks at depth {search.depth} and can still
        change. Nothing is stored until the search finishes.
      </p>
    </>
  );
}

/** Render a whole inferred list, grouping continuations under trunks. */
function InferLineList({
  lines,
  adopting,
  onPreview,
  onAdopt,
}: {
  lines: InferredLine[];
  adopting: boolean;
  onPreview: (p: ScrubPreview | null) => void;
  onAdopt: (l: InferredLine) => void;
}) {
  const shared = continuationDepths(lines);
  return (
    <>
      {lines.map((l, i) => (
        <InferLineRow
          key={l.sans.join(" ")}
          line={l}
          rank={i + 1}
          shared={shared[i]}
          adopting={adopting}
          onPreview={onPreview}
          onAdopt={() => onAdopt(l)}
        />
      ))}
    </>
  );
}

interface TriageViewProps {
  treatment?: BoardTreatment;
  /** Open a database game at a ply (deviations deep-link to the spot). */
  onOpenGameAt: (gameId: number, ply: number) => void;
  /** Adoption creates SRS cards — let the shell refresh its due badges. */
  onCountsChanged?: () => void;
  /** Identity lives on the Profile page — links point there. */
  onNavigate?: (view: "profile") => void;
}

export default function TriageView({
  treatment = "walnut",
  onOpenGameAt,
  onCountsChanged,
  onNavigate,
}: TriageViewProps) {
  /** Canonical identity (null = still asking; "" = app doesn't know you
   * yet). Identity is configured on the Profile page ONLY (2026-07-30
   * maintainer ruling: asking for a name here was dumb). */
  const [selfName, setSelfName] = useState<string | null>(null);
  const [report, setReport] = useState<TriageReport | null>(null);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [color, setColor] = useState<"white" | "black">("white");
  const [sel, setSel] = useState<Selection | null>(null);


  const [extStatus, setExtStatus] = useState<ExtensionStatus | null>(null);
  const [extError, setExtError] = useState<string | null>(null);
  const [adoptMsg, setAdoptMsg] = useState<string | null>(null);
  const [adopting, setAdopting] = useState(false);

  /* ---- inferred-repertoire suggestion flow (card-less colors) ---- */
  const [inferred, setInferred] = useState<InferredRepertoire | null>(null);
  const [inferring, setInferring] = useState(false);
  const [inferError, setInferError] = useState<string | null>(null);
  /** Name forms searched, shown when the identity has zero games. */
  const [identityForms, setIdentityForms] = useState<string[] | null>(null);
  const [inferMsg, setInferMsg] = useState<string | null>(null);
  /** The first report of the session picks the tab (a color that has
   * cards — never a dead tab); an explicit toggle is never overridden. */
  const colorAutoPicked = useRef(false);

  /* ---- declared-vs-played state (2026-07-30 v2) ---- */
  /** Reality panels dismissed with "Keep training the cards" — session
   * state only (keyed by position FEN); the deviation then lists normally. */
  const [dismissed, setDismissed] = useState<string[]>([]);
  /** Whole-opening-hole inference, one hole at a time (keyed by FEN). */
  const [holeInfer, setHoleInfer] = useState<{
    fen: string;
    loading: boolean;
    inf: InferredRepertoire | null;
    error: string | null;
  } | null>(null);
  /** Board-played answer awaiting explicit confirmation — never silently
   * written (keyed to the position it was played from). */
  const [pendingAnswer, setPendingAnswer] = useState<{ fen: string; san: string } | null>(null);

  /** Hover-scrub line preview (2026-07-30 field request): while non-null
   * the aside board shows this position instead of the selected item's,
   * read-only. Every prospective-line surface feeds the same state. */
  const [preview, setPreview] = useState<ScrubPreview | null>(null);


  /* ---- build the report (auto-runs: visiting the page = current truth) ---- */
  const run = useCallback(async (p: string) => {
    if (p.trim() === "") return;
    setBuilding(true);
    setError(null);
    setSel(null);
    setHoleInfer(null);
    setPendingAnswer(null);
    setPreview(null);
    try {
      const r = await triageReport(p.trim());
      setReport(r);
      if (!colorAutoPicked.current) {
        colorAutoPicked.current = true;
        setColor(defaultTriageColor(r));
      }
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setBuilding(false);
    }
  }, []);

  useEffect(() => {
    selfPlayerGet()
      .then((name) => {
        setSelfName(name ?? "");
        if (name) void run(name);
      })
      .catch(() => setSelfName(""));
  }, [run]);

  const ct: ColorTriage | null = report ? (color === "white" ? report.white : report.black) : null;

  const select = useCallback((kind: TriageKind, item: TriageItem) => {
    setSel({ kind, item });
    setExtStatus(null);
    setExtError(null);
    setAdoptMsg(null);
    setPendingAnswer(null);
    setPreview(null);
  }, []);

  /* ---- extension status: fetch on selection, poll while queued/running ---- */
  const selFen = sel && sel.kind !== "deviation" ? sel.item.fen : null;
  const pollBusy = extStatus?.jobStatus === "pending" || extStatus?.jobStatus === "running";
  // A running search reports every quarter second; waiting in a queue does
  // not change that fast, so only the live case polls quickly.
  const pollMs = extStatus?.jobStatus === "running" ? 1000 : 2500;
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
    const t = setInterval(fetchStatus, pollMs);
    return () => {
      stale = true;
      clearInterval(t);
    };
  }, [selFen, pollBusy, pollMs]);

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
        search: null,
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

  /* ---- inference: runs whenever the selected color has no cards ---- */
  const needInfer = report !== null && ct !== null && !ct.hasCards;
  const reportPlayer = report?.player ?? null;
  useEffect(() => {
    if (!needInfer || reportPlayer === null) {
      setInferred(null);
      setInferError(null);
      setInferring(false);
      return;
    }
    let stale = false;
    setInferring(true);
    setInferError(null);
    setInferred(null);
    setIdentityForms(null);
    triageInferRepertoire(reportPlayer, color)
      .then(async (inf) => {
        if (stale) return;
        setInferred(inf);
        if (inf.gamesScanned === 0) {
          // Zero games for the identity: name the forms actually searched.
          try {
            const forms = await identityGroup(reportPlayer);
            if (!stale) setIdentityForms(forms.map((f) => f.name));
          } catch {
            if (!stale) setIdentityForms([reportPlayer]);
          }
        }
      })
      .catch((e) => {
        if (!stale) setInferError(String(e));
      })
      .finally(() => {
        if (!stale) setInferring(false);
      });
    return () => {
      stale = true;
    };
  }, [needInfer, reportPlayer, color]);

  /** Adopt inferred lines (trainAddLine per line), then re-run the
   * triage so the user lands on actual triage points. */
  const adoptInferred = useCallback(
    async (lines: InferredLine[]) => {
      if (adopting || lines.length === 0) return;
      setAdopting(true);
      setInferMsg(null);
      try {
        let added = 0;
        let existing = 0;
        let repName = "";
        for (const l of lines) {
          const res = await trainAddLine(color, l.sans);
          added += res.cardsAdded;
          existing += res.cardsExisting;
          repName = res.repertoire;
        }
        setInferMsg(
          `Adopted ${lines.length} line${lines.length === 1 ? "" : "s"} into "${repName}": ` +
            `${added} new card${added === 1 ? "" : "s"}, ${existing} position${
              existing === 1 ? "" : "s"
            } already covered.`,
        );
        onCountsChanged?.();
        if (selfName) await run(selfName); // land on the real triage points
      } catch (e) {
        setInferMsg(`Adoption failed: ${e}`);
      } finally {
        setAdopting(false);
      }
    },
    [color, adopting, onCountsChanged, run, selfName],
  );

  /** Adopt what a reality-check panel shows the user really plays:
   * trainAddLine in REPLACE mode (the conflicting card is rewritten to
   * the played move — otherwise first-card-wins would keep the old card
   * and the panel would return forever), then re-run the triage. */
  const adoptPlayed = useCallback(
    async (lines: InferredLine[]) => {
      if (adopting || lines.length === 0) return;
      setAdopting(true);
      setInferMsg(null);
      try {
        let added = 0;
        let existing = 0;
        let replaced = 0;
        let repName = "";
        for (const l of lines) {
          const res = await trainAddLine(color, l.sans, undefined, undefined, true);
          added += res.cardsAdded;
          existing += res.cardsExisting;
          replaced += res.cardsReplaced;
          repName = res.repertoire;
        }
        const bits = [`${added} new card${added === 1 ? "" : "s"}`];
        if (replaced > 0) {
          bits.push(`${replaced} card${replaced === 1 ? "" : "s"} rewritten to your move`);
        }
        bits.push(`${existing} position${existing === 1 ? "" : "s"} already covered`);
        setInferMsg(`Adopted what you play into "${repName}": ${bits.join(", ")}.`);
        onCountsChanged?.();
        if (selfName) await run(selfName);
      } catch (e) {
        setInferMsg(`Adoption failed: ${e}`);
      } finally {
        setAdopting(false);
      }
    },
    [color, adopting, onCountsChanged, run, selfName],
  );

  /** "[Infer from your games]" on a whole-opening hole: rooted inference
   * after the opponent's move (static walk — no engine). */
  const inferHole = useCallback(
    async (it: TriageItem) => {
      if (reportPlayer === null) return;
      setHoleInfer({ fen: it.fen, loading: true, inf: null, error: null });
      try {
        const inf = await triageInferFrom(reportPlayer, color, lineSans(it.line));
        setHoleInfer({ fen: it.fen, loading: false, inf, error: null });
      } catch (e) {
        setHoleInfer({ fen: it.fen, loading: false, inf: null, error: String(e) });
      }
    },
    [reportPlayer, color],
  );

  /* ---- "I know my answer": play the move on the position board ---- */

  /** A selected gap (any kind) or reality-check deviation accepts the
   * user's own move on the aside board; everything else stays read-only. */
  const answerable =
    sel !== null && (sel.kind === "gap" || (sel.kind === "deviation" && sel.item.realityCheck));

  const answerMove = useCallback(
    (orig: string, dest: string, role?: "queen" | "rook" | "bishop" | "knight") => {
      if (!sel) return;
      const san = sanForBoardMove(sel.item.fen, orig, dest, role);
      if (san) setPendingAnswer({ fen: sel.item.fen, san });
    },
    [sel],
  );
  const answerMoveRef = useRef(answerMove);
  answerMoveRef.current = answerMove;
  const promo = usePromotionPicker((orig, dest, role) =>
    answerMoveRef.current(orig, dest, role),
  );

  const movable: BoardMovable | undefined =
    answerable && sel
      ? {
          color,
          dests: trainDests(sel.item.fen),
          onMove: (orig, dest) => {
            if (!promo.guard(sel.item.fen, orig, dest)) answerMoveRef.current(orig, dest);
          },
        }
      : undefined;

  /** Confirmed board answer → trainAddLine with the item's path + the
   * move (replace mode only at a deviation, where a card conflicts). */
  const adoptAnswer = useCallback(async () => {
    if (!sel || !pendingAnswer || pendingAnswer.fen !== sel.item.fen || adopting) return;
    setAdopting(true);
    setAdoptMsg(null);
    try {
      const sans = answerLineSans(sel.item, pendingAnswer.san);
      const replace = sel.kind === "deviation";
      const res = await trainAddLine(color, sans, undefined, undefined, replace);
      const bits = [`${res.cardsAdded} new card${res.cardsAdded === 1 ? "" : "s"}`];
      if (res.cardsReplaced > 0) {
        bits.push(
          `${res.cardsReplaced} card${res.cardsReplaced === 1 ? "" : "s"} rewritten to your move`,
        );
      }
      bits.push(
        `${res.cardsExisting} position${res.cardsExisting === 1 ? "" : "s"} already covered`,
      );
      setInferMsg(
        `Set ${pendingAnswer.san} as your answer in "${res.repertoire}": ${bits.join(", ")}.`,
      );
      setPendingAnswer(null);
      onCountsChanged?.();
      if (selfName) await run(selfName);
    } catch (e) {
      setAdoptMsg(`Adoption failed: ${e}`);
    } finally {
      setAdopting(false);
    }
  }, [sel, pendingAnswer, adopting, color, onCountsChanged, run, selfName]);

  const extension = extStatus?.extension ?? null;

  /** Reality panels still standing this session (not dismissed). */
  const realityItems =
    ct && ct.hasCards ? realityDeviations(ct).filter((d) => !dismissed.includes(d.fen)) : [];
  /** Whole-opening holes get their own rows; the ranked lists keep the
   * remaining deviations and the real in-book gaps. */
  const holes = ct && ct.hasCards ? wholeOpeningGaps(ct) : [];
  const listCt: ColorTriage | null = ct
    ? {
        ...ct,
        deviations: ct.deviations.filter((d) => !realityItems.some((r) => r.fen === d.fen)),
        gaps: inBookGaps(ct),
      }
    : null;

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
                colorAutoPicked.current = true;
                setColor("white");
                setSel(null);
                setHoleInfer(null);
                setPendingAnswer(null);
                setPreview(null);
              }}
            >
              as White
            </button>
            <button
              className={color === "black" ? "cur" : ""}
              onClick={() => {
                colorAutoPicked.current = true;
                setColor("black");
                setSel(null);
                setHoleInfer(null);
                setPendingAnswer(null);
                setPreview(null);
              }}
            >
              as Black
            </button>
          </div>
        }
      />
      <div className="triage-body">
        <div className="triage-main">
          {selfName === "" && (
            <div className="triage-setup">
              <p className="triage-footnote">
                Kibitz doesn&apos;t know who you are yet — build your profile once and every
                self-facing screen (triage, the lab, Home) knows whose games to read.
              </p>
              <button className="btn-primary" onClick={() => onNavigate?.("profile")}>
                Set up on Profile
              </button>
            </div>
          )}
          {selfName && (
            <div className="triage-identity">
              for <strong>{selfName}</strong>
              {building && <span className="dim"> · walking your games…</span>}
              <button className="linklike" onClick={() => onNavigate?.("profile")}>
                change on Profile
              </button>
            </div>
          )}
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
                {report.player} as {colorName(color)}
                {ct.hasCards
                  ? ` · ${ct.gamesScanned} game${ct.gamesScanned === 1 ? "" : "s"} scanned`
                  : ""}{" "}
                · {triageSummary(ct)}
              </div>
              {inferMsg && <div className="triage-adopt-msg">{inferMsg}</div>}
              {!ct.hasCards ? (
                <div className="triage-infer">
                  {inferring && (
                    <div className="triage-ext-progress">
                      Reading your {colorName(color)} games for the lines you already play…
                    </div>
                  )}
                  {inferError && <div className="error">{inferError}</div>}
                  {inferred && inferred.gamesScanned === 0 && (
                    <div className="pf2-empty">
                      No {colorName(color)} games found for this identity
                      {identityForms && identityForms.length > 0
                        ? ` (searched: ${identityForms.join(", ")})`
                        : ""}
                      . If you play under another handle — your chess.com username, say — it may
                      not be declared as you yet: add it on the Profile screen&rsquo;s INCLUDES
                      strip, then re-run the triage.
                    </div>
                  )}
                  {inferred && inferred.gamesScanned > 0 && inferred.lines.length === 0 && (
                    <div className="pf2-empty">
                      Walked {inferred.gamesScanned} {colorName(color)} game
                      {inferred.gamesScanned === 1 ? "" : "s"}, but no opening line repeats in
                      enough of them to suggest (3+ games). Add lines from the Game view
                      (&ldquo;→ repertoire&rdquo;) or import a PGN study instead.
                    </div>
                  )}
                  {inferred && inferred.lines.length > 0 && (
                    <>
                      <div className="triage-infer-headline">
                        No {colorName(color)} repertoire yet — but your games already show what
                        you play:
                      </div>
                      <InferLineList
                        lines={inferred.lines}
                        adopting={adopting}
                        onPreview={setPreview}
                        onAdopt={(l) => void adoptInferred([l])}
                      />
                      <button
                        className="btn-primary"
                        disabled={adopting}
                        onClick={() => void adoptInferred(inferred.lines)}
                      >
                        {adopting
                          ? "Adopting…"
                          : `Adopt all ${inferred.lines.length} line${
                              inferred.lines.length === 1 ? "" : "s"
                            }`}
                      </button>
                      <p className="triage-footnote">
                        Inferred from your {inferred.gamesScanned} most recent {colorName(color)}{" "}
                        games: the tree of the openings they repeat, following every branch at
                        least 3 games support, each line ending on a move of yours. Indented
                        lines go deeper into the line above them. Adopting creates SRS cards from
                        your moves and re-runs the triage automatically.
                      </p>
                    </>
                  )}
                </div>
              ) : (
                <>
                  {realityItems.map((it) => (
                    <div className="triage-reality" key={it.fen}>
                      <div className="triage-infer-headline">{realityHeadline(it)}</div>
                      <InferLineList
                        lines={it.inferredLines}
                        adopting={adopting}
                        onPreview={setPreview}
                        onAdopt={(l) => void adoptPlayed([l])}
                      />
                      <div className="triage-reality-actions">
                        <button
                          className="btn-primary"
                          disabled={adopting || it.inferredLines.length === 0}
                          onClick={() => void adoptPlayed(it.inferredLines)}
                        >
                          {adopting ? "Adopting…" : "Adopt what you play"}
                        </button>
                        <button
                          className="btn-secondary"
                          onClick={() => setDismissed((d) => [...d, it.fen])}
                        >
                          Keep training the cards
                        </button>
                        <button className="btn-secondary" onClick={() => select("deviation", it)}>
                          I know my answer — play it on the board
                        </button>
                      </div>
                      <p className="triage-footnote">
                        Adopting rewrites the conflicting card to the move you actually play (its
                        training restarts fresh) and adds cards for the rest of each line. Keeping
                        the cards leaves this listed as a normal deviation for this session.
                      </p>
                    </div>
                  ))}
                  {holes.length > 0 && (
                    <div className="triage-section">
                      <div className="triage-strip-title">
                        WHOLE-OPENING HOLES — NO BOOK AT ALL AFTER THESE
                      </div>
                      {holes.map((it) => (
                        <div key={it.fen}>
                          <div
                            className={`triage-row triage-hole${
                              sel?.item.fen === it.fen ? " sel" : ""
                            }`}
                          >
                            <button
                              type="button"
                              className="triage-hole-main"
                              onClick={() => select("gap", it)}
                            >
                              <span className="triage-line">{wholeGapLabel(it)}</span>
                              <span className="triage-caption">
                                {it.eco && it.openingName
                                  ? `${it.eco} ${it.openingName}`
                                  : "the opponent's first move — your book has no answer"}
                                {it.hasExtension ? " · engine lines ready" : ""}
                              </span>
                            </button>
                            <button
                              className="btn-secondary"
                              disabled={holeInfer?.loading && holeInfer.fen === it.fen}
                              onClick={() => {
                                select("gap", it);
                                void inferHole(it);
                              }}
                            >
                              Infer from your games
                            </button>
                          </div>
                          {holeInfer?.fen === it.fen && (
                            <div className="triage-hole-infer">
                              {holeInfer.loading && (
                                <div className="triage-ext-progress">
                                  Reading your games for what you already play here…
                                </div>
                              )}
                              {holeInfer.error && <div className="error">{holeInfer.error}</div>}
                              {holeInfer.inf && holeInfer.inf.lines.length === 0 && (
                                <div className="pf2-empty">
                                  Walked {holeInfer.inf.gamesScanned} game
                                  {holeInfer.inf.gamesScanned === 1 ? "" : "s"} here, but no line
                                  repeats in enough of them to suggest (3+ games). Play
                                  your answer on the board instead, or extend with the engine.
                                </div>
                              )}
                              {holeInfer.inf && (
                                <InferLineList
                                  lines={holeInfer.inf.lines}
                                  adopting={adopting}
                                  onPreview={setPreview}
                                  onAdopt={(l) => void adoptInferred([l])}
                                />
                              )}
                              {holeInfer.inf && holeInfer.inf.lines.length > 0 && (
                                <button
                                  className="btn-primary"
                                  disabled={adopting}
                                  onClick={() => {
                                    const lines = holeInfer.inf?.lines ?? [];
                                    void adoptInferred(lines);
                                  }}
                                >
                                  {adopting
                                    ? "Adopting…"
                                    : `Adopt all ${holeInfer.inf.lines.length} line${
                                        holeInfer.inf.lines.length === 1 ? "" : "s"
                                      }`}
                                </button>
                              )}
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                  {listCt && (
                    <TriageLists
                      ct={listCt}
                      selectedFen={sel?.item.fen ?? null}
                      onSelect={select}
                    />
                  )}
                </>
              )}
            </>
          )}
        </div>

        <aside className="triage-aside">
          <div className="triage-board-wrap">
            {/* A live scrub preview drives the board read-only; null
             * restores the selected item's position and movability. */}
            <Board
              fen={preview?.fen ?? sel?.item.fen ?? START_FEN}
              lastMove={preview?.lastMove ?? undefined}
              orientation={color}
              treatment={treatment}
              size={360}
              movable={preview ? undefined : movable}
            />
            {promo.element}
            {preview && <div className="scrub-caption">after {preview.label}</div>}
          </div>
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

              {answerable && pendingAnswer && pendingAnswer.fen === sel.item.fen ? (
                <div className="triage-answer">
                  <div className="triage-answer-copy">
                    {answerConfirmCopy(sel.item, pendingAnswer.san)}
                  </div>
                  <div className="triage-reality-actions">
                    <button
                      className="btn-primary"
                      disabled={adopting}
                      onClick={() => void adoptAnswer()}
                    >
                      Set as my answer
                    </button>
                    <button className="btn-secondary" onClick={() => setPendingAnswer(null)}>
                      Cancel
                    </button>
                  </div>
                </div>
              ) : answerable ? (
                <p className="triage-footnote">
                  Know your answer? Play it on the board and confirm — nothing is written until
                  you do.
                </p>
              ) : null}

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
                          <ScrubLine
                            className="triage-ext-sans"
                            sans={line.sans}
                            startFen={extension.fen}
                            onPreview={setPreview}
                          />
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
                    <>
                      <div className="triage-ext-progress">
                        {extStatus.search
                          ? `Engine searching — ${searchProgressLabel(extStatus.search)}`
                          : "Engine starting the search…"}
                      </div>
                      {extStatus.search !== null && (
                        <LiveSearchPanel search={extStatus.search} onPreview={setPreview} />
                      )}
                    </>
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
