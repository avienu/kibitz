/**
 * Explain panel (design/handoff-1 §D + round-3 change note): a BOUNDED
 * three-row panel — header / scrolling prose / pinned foot. Summary
 * first: only the leading finding renders until the foot's expander
 * opens the rest (per position; GameView resets it on every step). The
 * header's caret collapses the whole body for fast stepping. Neither
 * state touches the board: the overlay always shows the union of ALL
 * findings — collapsing hides prose, never evidence.
 */
import {
  hiddenFindingIndices,
  selectionNote,
  sentenceOpacity,
  suggestionTitle,
  BLOCK_HORIZON,
  type ExplainBlockKind,
  type ExplanationJson,
  type Horizon,
  type Voice,
} from "./lib/gameView";
import { visibleChips, type VerificationState } from "./lib/verifyChips";

const KIND_LABEL: Record<ExplainBlockKind, string> = {
  alert: "TACTICAL ALERT",
  imbalance: "IMBALANCE",
  plan: "PLAN",
  scheme: "LONG-TERM PLAN",
};

/** Section headings for the horizon groups, in reading order. */
const HORIZON_ORDER: { horizon: Horizon; label: string }[] = [
  { horizon: "now", label: "NOW" },
  { horizon: "next", label: "NEXT" },
  { horizon: "long", label: "LONG-TERM" },
];

interface ExplainPanelProps {
  explanation: ExplanationJson | null;
  explaining: boolean;
  /** CONSIDER-chip verification (run 11): drives the pending affordance
   * and which chips render at all (lib/verifyChips.ts). */
  verification: VerificationState | null;
  voice: Voice;
  onVoice: (v: Voice) => void;
  hoverSentence: number | null;
  onHoverSentence: (i: number | null) => void;
  selectedSquare: string | null;
  onExplain: () => void;
  /** Plies (of the current game) that already have cached explanations. */
  explainedPlies: number[];
  /** Summary-first (round-3 change note): false = only the first finding
   * renders. Owned by GameView; resets on every move step. */
  findingsExpanded: boolean;
  onToggleFindings: () => void;
  /** Whole-body collapse (the header caret) — the fast-stepping state.
   * Survives move steps. */
  collapsed: boolean;
  onToggleCollapsed: () => void;
}

export default function ExplainPanel({
  explanation,
  explaining,
  verification,
  voice,
  onVoice,
  hoverSentence,
  onHoverSentence,
  selectedSquare,
  onExplain,
  explainedPlies,
  findingsExpanded,
  onToggleFindings,
  collapsed,
  onToggleCollapsed,
}: ExplainPanelProps) {
  const pill = explanation
    ? `${explanation.tag}${explanation.eval ? `  ${explanation.eval.display}` : ""}`
    : null;
  const pillBad = explanation?.tag === "FORCED MATE";

  const blocks = explanation?.blocks ?? [];
  const hidden = new Set(hiddenFindingIndices(blocks, findingsExpanded));
  // Count what is ACTUALLY hidden. Since run 12 the leading finding of
  // each horizon survives collapse, so "n more" is no longer "all but
  // the first" — deriving it from the hidden set keeps the label honest.
  const moreCount = hiddenFindingIndices(blocks, false).length;

  // CONSIDER chips (run 11): statically-marked chips stay hidden until
  // the engine clears them; refuted chips disappear; the pending
  // affordance shows only while a verification round-trip runs.
  const chips = visibleChips(explanation, verification);
  const verifying = verification?.kind === "running";

  const meta = explanation
    ? [
        verification?.kind === "done"
          ? "Static screen · candidates engine-checked"
          : verifying
            ? "Static screen · checking candidates…"
            : "Static screen · no engine spawned",
        `${voice === "coach" ? "Coach" : "Neutral"} voice · templates`,
        selectionNote(selectedSquare),
      ].join(" · ")
    : null;

  return (
    <section className={`explain-panel${collapsed ? " collapsed" : ""}`}>
      <header className="panel-header">
        <span className="panel-label">EXPLAIN</span>
        {pill && <span className={`verdict-pill${pillBad ? " bad" : ""}`}>{pill}</span>}
        <span className="seg" role="group" aria-label="Narration voice">
          {(["coach", "neutral"] as const).map((v) => (
            <button
              key={v}
              className={voice === v ? "cur" : ""}
              onClick={() => onVoice(v)}
            >
              {v === "coach" ? "Coach" : "Neutral"}
            </button>
          ))}
        </span>
        <button
          className="explain-collapse"
          title={collapsed ? "Show the explanation" : "Collapse — evidence stays on the board"}
          aria-expanded={!collapsed}
          onClick={onToggleCollapsed}
        >
          {collapsed ? "▸" : "▾"}
        </button>
      </header>

      {!collapsed &&
        (explanation ? (
          <>
            <div className="explain-body">
              <p className="explain-headline">{explanation.headline[voice]}</p>
              {HORIZON_ORDER.map(({ horizon, label }) => {
                const rows = explanation.blocks
                  .map((b, i) => ({ b, i }))
                  .filter(({ b, i }) => !hidden.has(i) && BLOCK_HORIZON[b.kind] === horizon);
                // A horizon with nothing in it is not a heading — an empty
                // "LONG-TERM" label reads as a promise the engine broke.
                if (rows.length === 0) return null;
                return (
                  <section key={horizon} className={`horizon horizon-${horizon}`}>
                    <h4 className="horizon-label">{label}</h4>
                    {rows.map(({ b, i }) => (
                      <div
                        key={i}
                        className={`sentence sentence-${b.kind}${hoverSentence === i ? " hovered" : ""}`}
                        style={{ opacity: sentenceOpacity(b, selectedSquare) }}
                        onMouseEnter={() => onHoverSentence(i)}
                        onMouseLeave={() => onHoverSentence(null)}
                      >
                        <span className="sentence-dot" aria-hidden />
                        <span className="sentence-main">
                          <span className="sentence-kind">{KIND_LABEL[b.kind]}</span>
                          <span className="sentence-prose">{b.text[voice]}</span>
                        </span>
                      </div>
                    ))}
                  </section>
                );
              })}
              {chips.length > 0 && (
                <div className="consider-block">
                  <span className="consider-label">CONSIDER</span>
                  {verifying && <span className="consider-verifying">verifying…</span>}
                  <div className="consider-chips">
                    {chips.map(({ s, index: j, pending }, pos) => {
                      const idx = explanation.blocks.length + j;
                      return (
                        <span
                          key={s.uci}
                          className={`consider-chip${pos === 0 ? " top" : ""}${
                            s.prophylactic ? " prophylactic" : ""
                          }${pending ? " pending" : ""}${hoverSentence === idx ? " hovered" : ""}`}
                          title={suggestionTitle(s)}
                          onMouseEnter={() => onHoverSentence(idx)}
                          onMouseLeave={() => onHoverSentence(null)}
                        >
                          {s.san}
                        </span>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
            <footer className="explain-foot">
              {moreCount > 0 && (
                <button className="explain-expander" onClick={onToggleFindings}>
                  {findingsExpanded
                    ? "▴ summary only"
                    : `▾ ${moreCount} more finding${moreCount === 1 ? "" : "s"} — evidence is already on the board`}
                </button>
              )}
              {meta && <div className="explain-meta">{meta}</div>}
            </footer>
          </>
        ) : (
          <div className="explain-body explain-empty">
            <p className="explain-empty-prose">
              No screen has fired on this position. Kibitz keeps the engine cold until you ask, or
              until a tactical screen actually trips.
            </p>
            <button className="btn-primary" onClick={onExplain} disabled={explaining}>
              {explaining ? "Explaining…" : "Explain position"}
              <span className="btn-key">E</span>
            </button>
            {explainedPlies.length > 0 && (
              <div className="explain-empty-note">
                Explanations cached for {explainedPlies.length === 1 ? "ply" : "plies"}{" "}
                {explainedPlies.join(", ")}.
              </div>
            )}
          </div>
        ))}
    </section>
  );
}
