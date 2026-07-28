/**
 * Explain panel (design/handoff-1 §D): headline + sentence blocks with
 * role dots, driving the board's evidence overlays through hover; empty
 * state with an explicit `Explain position (E)` action — the engine (and
 * even the static screen) runs only when asked.
 */
import {
  collapsedAlertIndices,
  MAX_VISIBLE_ALERTS,
  selectionNote,
  sentenceOpacity,
  suggestionTitle,
  type ExplainBlockKind,
  type ExplanationJson,
  type Voice,
} from "./lib/gameView";
import { visibleChips, type VerificationState } from "./lib/verifyChips";

const KIND_LABEL: Record<ExplainBlockKind, string> = {
  alert: "TACTICAL ALERT",
  imbalance: "IMBALANCE",
  plan: "PLAN",
};

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
  /** Alert collapse (audit #13): more than MAX_VISIBLE_ALERTS alert
   * sentences fold behind a "show N more" toggle. Owned by GameView so
   * the board's evidence union tracks what is visible. */
  alertsExpanded: boolean;
  onToggleAlerts: () => void;
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
  alertsExpanded,
  onToggleAlerts,
}: ExplainPanelProps) {
  const pill = explanation
    ? `${explanation.tag}${explanation.eval ? `  ${explanation.eval.display}` : ""}`
    : null;
  const pillBad = explanation?.tag === "FORCED MATE";

  // Alert collapse (audit #13): hidden indices when folded, and the block
  // index the toggle renders after (the last visible alert).
  const blocks = explanation?.blocks ?? [];
  const alertIndices = blocks.flatMap((b, i) => (b.kind === "alert" ? [i] : []));
  const hiddenCount = collapsedAlertIndices(blocks, false).length;
  const hidden = new Set(alertsExpanded ? [] : collapsedAlertIndices(blocks, false));
  const toggleAfter =
    hiddenCount > 0
      ? alertIndices[alertsExpanded ? alertIndices.length - 1 : MAX_VISIBLE_ALERTS - 1]
      : null;

  // CONSIDER chips (run 11): statically-marked chips stay hidden until
  // the engine clears them; refuted chips disappear; the pending
  // affordance shows only while a verification round-trip runs.
  const chips = visibleChips(explanation, verification);
  const verifying = verification?.kind === "running";

  return (
    <section className="explain-panel">
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
      </header>

      {explanation ? (
        <div className="explain-body">
          <p className="explain-headline">{explanation.headline[voice]}</p>
          {explanation.blocks.map((b, i) =>
            hidden.has(i) ? null : (
              <div key={i}>
                <div
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
                {i === toggleAfter && (
                  <button className="sentence-more" onClick={onToggleAlerts}>
                    {alertsExpanded
                      ? "show fewer alerts"
                      : `show ${hiddenCount} more alert${hiddenCount === 1 ? "" : "s"}`}
                  </button>
                )}
              </div>
            ),
          )}
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
          <footer className="explain-footer">
            <span>
              {verification?.kind === "done"
                ? "Static screen · candidates engine-checked"
                : verifying
                  ? "Static screen · checking candidates…"
                  : "Static screen · no engine spawned"}
            </span>
            <span>·</span>
            <span>{voice === "coach" ? "Coach" : "Neutral"} voice · templates</span>
            <span>·</span>
            <span>{selectionNote(selectedSquare)}</span>
          </footer>
        </div>
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
      )}
    </section>
  );
}
