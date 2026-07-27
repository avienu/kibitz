/**
 * First-run tour (design/handoff-2 §Help & first-run tour): one card per
 * rail group, anchored BESIDE the group it describes (measured from the
 * real rail's bounding boxes), never covering it. Shown on first run
 * (localStorage flag, unchanged) and replayable from Help.
 *
 * The card sequence/state machine is pure (lib/tour.ts) and unit-tested;
 * this component only measures and renders.
 */
import { useCallback, useEffect, useLayoutEffect, useReducer, useState } from "react";
import {
  TOUR_STEPS,
  initialTour,
  reduceTour,
  tourCounter,
  type TourAnchor,
} from "./lib/tour";

const TOUR_KEY = "silman.tourSeen";

/** True until the user dismisses the first-run overlay once. */
export function shouldShowFirstRun(): boolean {
  try {
    return localStorage.getItem(TOUR_KEY) === null;
  } catch {
    return false; // non-browser environment: never show
  }
}

export function markFirstRunSeen(): void {
  try {
    localStorage.setItem(TOUR_KEY, "1");
  } catch {
    // Nothing to persist outside a browser.
  }
}

interface FirstRunOverlayProps {
  /** Dismiss the overlay (the caller persists the seen flag). */
  onClose: () => void;
  /** Dismiss and open the user guide. */
  onOpenHelp: () => void;
}

/** Find the rail element a step anchors beside. */
function anchorElement(anchor: TourAnchor): Element | null {
  if (anchor === "header") return document.querySelector(".rail .rail-header");
  if (anchor === "footer") return document.querySelector(".rail .rail-footer");
  const groups = document.querySelectorAll(".rail .rail-group");
  const idx = { study: 0, coach: 1, train: 2, data: 3 }[anchor];
  return groups[idx] ?? null;
}

const CARD_WIDTH = 300;
const CARD_EST_HEIGHT = 190;

export default function FirstRunOverlay({ onClose, onOpenHelp }: FirstRunOverlayProps) {
  const [state, dispatch] = useReducer(
    (s: Parameters<typeof reduceTour>[0], a: Parameters<typeof reduceTour>[1]) => reduceTour(s, a),
    undefined,
    initialTour,
  );
  const [pos, setPos] = useState<{ top: number; left: number; caretTop: number } | null>(null);

  const step = TOUR_STEPS[state.step];
  const last = state.step === TOUR_STEPS.length - 1;

  useEffect(() => {
    if (state.done) onClose();
  }, [state.done, onClose]);

  // Anchor beside the rail group: card left = rail's right edge + gap,
  // card top follows the group's top (clamped into the viewport) — the
  // card never covers the rail itself.
  const measure = useCallback(() => {
    const rail = document.querySelector(".rail");
    const anchor = anchorElement(step.anchor);
    if (!rail || !anchor) {
      setPos(null);
      return;
    }
    const railBox = rail.getBoundingClientRect();
    const box = anchor.getBoundingClientRect();
    const top = Math.max(12, Math.min(box.top, window.innerHeight - CARD_EST_HEIGHT - 12));
    setPos({
      left: railBox.right + 14,
      top,
      caretTop: Math.max(14, box.top + box.height / 2 - top),
    });
  }, [step.anchor]);

  useLayoutEffect(() => {
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [measure]);

  return (
    <div className="tour-layer">
      <div
        className="tour-card tour-float"
        role="dialog"
        aria-label={`First-run tour, card ${tourCounter(state.step)}`}
        style={
          pos
            ? { top: pos.top, left: pos.left, width: CARD_WIDTH }
            : { top: 80, left: 232, width: CARD_WIDTH }
        }
      >
        <span className="tour-caret" style={pos ? { top: pos.caretTop } : undefined} />
        <div className="tour-card-head">
          <span className="tour-tag">FIRST-RUN TOUR</span>
          <span className="flex-spacer" />
          <span className="tour-count">{tourCounter(state.step)}</span>
        </div>
        <p className="tour-body">
          <b>{step.title}</b> {step.body}
        </p>
        <div className="tour-actions">
          <button className="btn-primary" onClick={() => dispatch({ type: "next" })}>
            {last ? "Finish" : "Next"}
          </button>
          <button className="btn-ghost" onClick={() => dispatch({ type: "skip" })}>
            Skip tour
          </button>
          {last && (
            <button className="btn-ghost" onClick={onOpenHelp}>
              Open the user guide
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
