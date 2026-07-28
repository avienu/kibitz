/**
 * Explain panel (design/handoff-1 §D): headline + sentence blocks with
 * role dots, driving the board's evidence overlays through hover; empty
 * state with an explicit `Explain position (E)` action — the engine (and
 * even the static screen) runs only when asked.
 */
import {
  selectionNote,
  sentenceOpacity,
  suggestionTitle,
  type ExplainBlockKind,
  type ExplanationJson,
  type Voice,
} from "./lib/gameView";

const KIND_LABEL: Record<ExplainBlockKind, string> = {
  alert: "TACTICAL ALERT",
  imbalance: "IMBALANCE",
  plan: "PLAN",
};

interface ExplainPanelProps {
  explanation: ExplanationJson | null;
  explaining: boolean;
  voice: Voice;
  onVoice: (v: Voice) => void;
  hoverSentence: number | null;
  onHoverSentence: (i: number | null) => void;
  selectedSquare: string | null;
  onExplain: () => void;
  /** Plies (of the current game) that already have cached explanations. */
  explainedPlies: number[];
}

export default function ExplainPanel({
  explanation,
  explaining,
  voice,
  onVoice,
  hoverSentence,
  onHoverSentence,
  selectedSquare,
  onExplain,
  explainedPlies,
}: ExplainPanelProps) {
  const pill = explanation
    ? `${explanation.tag}${explanation.eval ? `  ${explanation.eval.display}` : ""}`
    : null;
  const pillBad = explanation?.tag === "FORCED MATE";

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
          {explanation.blocks.map((b, i) => (
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
          {(explanation.suggestions?.length ?? 0) > 0 && (
            <div className="consider-block">
              <span className="consider-label">CONSIDER</span>
              <div className="consider-chips">
                {explanation.suggestions!.map((s, j) => {
                  const idx = explanation.blocks.length + j;
                  return (
                    <span
                      key={s.uci}
                      className={`consider-chip${j === 0 ? " top" : ""}${
                        s.prophylactic ? " prophylactic" : ""
                      }${hoverSentence === idx ? " hovered" : ""}`}
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
            <span>Static screen · no engine spawned</span>
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
