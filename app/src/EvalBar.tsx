/**
 * Eval bar (design/handoff-1 §C + deliverable 2a): vertical track left of
 * the board. The track is Black's share (dark in both themes); the fill,
 * anchored bottom, is White's. States: cp, NO-DATA (empty track, muted
 * dash — never a fake 0.0) and MATE (pinned 94/6, readout "#N" in the
 * winning side's colour: White → --accent, Black → --bad).
 */
import { evalBarView, type EvalBarView } from "./lib/gameView";
import type { AnalysisRow } from "./lib/analyses";

interface EvalBarProps {
  /** The analyses row selected for the current ply (fresh preferred). */
  row: AnalysisRow | null;
  /** Signed mate distance (White POV) when an explanation supplies one. */
  mate?: number | null;
  /** Track height (matches the board grid edge). */
  height: number;
}

function readoutColor(v: EvalBarView): string | undefined {
  if (v.state !== "mate") return undefined;
  return v.winner === "white" ? "var(--accent)" : "var(--bad)";
}

export default function EvalBar({ row, mate, height }: EvalBarProps) {
  const v = evalBarView(row, mate);
  return (
    <div className="evalbar" title={v.tooltip}>
      <div className="evalbar-label">EVAL</div>
      <div className="evalbar-track" style={{ height }}>
        {v.fillPct !== null && (
          <div className="evalbar-fill" style={{ height: `${v.fillPct}%` }} />
        )}
      </div>
      <div
        className={`evalbar-readout${v.state === "no-data" ? " nodata" : ""}`}
        style={{ color: readoutColor(v) }}
      >
        {v.readout}
      </div>
    </div>
  );
}
