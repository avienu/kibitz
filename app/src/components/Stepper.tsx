/**
 * Stepper — round-2 shared component 4 of 5 (design/handoff-2 §pattern
 * budget). Numbered workflow chips in a header strip; each passed step
 * shows its chosen value; the active step carries `var(--panel3)` with an
 * accent numeral badge; steps are freely re-clickable (backward always,
 * forward as far as the caller allows). Used by Opponent prep.
 *
 * Props contract (stable):
 * - `steps`: ordered chips; `value` is the choice shown once made
 *   ("R. Halvorsen", "as Black", …) — null/undefined until chosen.
 * - `active`: index of the current step.
 * - `onSelect(i)`: chip click; the caller decides whether a forward jump
 *   is allowed (typically only onto reached steps).
 */

export interface StepperStep {
  label: string;
  /** The chosen value, shown once the step has been passed. */
  value?: string | null;
}

export interface StepperProps {
  steps: readonly StepperStep[];
  active: number;
  onSelect: (index: number) => void;
}

export default function Stepper({ steps, active, onSelect }: StepperProps) {
  return (
    <div className="stepper">
      {steps.map((s, i) => (
        <button
          key={s.label}
          type="button"
          className={`stepper-chip${i === active ? " active" : ""}`}
          onClick={() => onSelect(i)}
        >
          <span className={`stepper-num${i === active ? " accent" : ""}`}>{i + 1}</span>
          <span className="stepper-label">{s.label}</span>
          {s.value != null && s.value !== "" && (
            <span className="stepper-value">{s.value}</span>
          )}
        </button>
      ))}
    </div>
  );
}
