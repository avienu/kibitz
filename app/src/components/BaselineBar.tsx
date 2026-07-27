/**
 * BaselineBar — round-2 shared component 3 of 5 (design/handoff-2 §pattern
 * budget). 7px track on `var(--panel3)`, value fill in `var(--good)` or
 * `var(--bad)` at 0.75 opacity, optional 1px baseline tick at the peer
 * value. Used by Structure report, Prep fingerprint, Endgame tiers and
 * Woodpecker — build once, no per-screen forks.
 *
 * Props contract (stable):
 * - `fraction`: fill 0..1 (clamped).
 * - `tone`: "good" | "bad" — which semantic hue fills the bar.
 * - `baseline`: optional 0..1 position of the 1px peer tick; omit for none.
 */

export interface BaselineBarProps {
  fraction: number;
  tone: "good" | "bad";
  baseline?: number | null;
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

export default function BaselineBar({ fraction, tone, baseline }: BaselineBarProps) {
  return (
    <div className="baseline-bar">
      <span
        className={`baseline-bar-fill ${tone}`}
        style={{ width: `${clamp01(fraction) * 100}%` }}
      />
      {baseline != null && (
        <span className="baseline-bar-tick" style={{ left: `${clamp01(baseline) * 100}%` }} />
      )}
    </div>
  );
}
