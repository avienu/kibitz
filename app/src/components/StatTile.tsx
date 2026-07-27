/**
 * StatTile — round-2 shared component 2 of 5 (design/handoff-2 §pattern
 * budget). 12px padding, `var(--panel2)`, radius 7; mono value at
 * 700 20–24px; mono caption 9.5px/0.14em in `var(--faint)`.
 * Used by Profile (phase accuracy, conversion), SRS session aside and the
 * Home band — build once, no per-screen forks.
 *
 * Props contract (stable):
 * - `caption`: the mono uppercase label above the value.
 * - `value`: the big mono numeral (string keeps formatting honest).
 * - `unit`: optional small UI-font unit beside the value ("ACPL").
 * - `delta`: optional signed peer delta beside the value, toned.
 * - `note`: optional small line under the value (error breakdown etc.).
 * - `selected` + `onClick`: tiles used as claim controls (Profile's
 *   "every number is a control") — clickable tiles get hover/selected
 *   states and re-target the evidence pane.
 */
import type { ReactNode } from "react";

export interface StatTileProps {
  caption: string;
  value: ReactNode;
  unit?: string;
  delta?: { text: string; tone: "good" | "bad" | "dim" };
  note?: ReactNode;
  selected?: boolean;
  onClick?: () => void;
}

export default function StatTile({
  caption,
  value,
  unit,
  delta,
  note,
  selected,
  onClick,
}: StatTileProps) {
  const cls =
    "stat-tile" + (onClick ? " clickable" : "") + (selected ? " selected" : "");
  const body = (
    <>
      <div className="stat-tile-caption">{caption}</div>
      <div className="stat-tile-row">
        <span className="stat-tile-value">{value}</span>
        {unit && <span className="stat-tile-unit">{unit}</span>}
        {delta && <span className={`stat-tile-delta ${delta.tone}`}>{delta.text}</span>}
      </div>
      {note && <div className="stat-tile-note">{note}</div>}
    </>
  );
  return onClick ? (
    <button type="button" className={cls} onClick={onClick}>
      {body}
    </button>
  ) : (
    <div className={cls}>{body}</div>
  );
}
