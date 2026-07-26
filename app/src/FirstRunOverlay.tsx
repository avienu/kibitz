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

/**
 * One-time orientation overlay (run-5 item 6, discoverability): points out
 * the nav-rail groups and where Help lives. Shown on first launch only;
 * the seen flag persists in localStorage.
 */
export default function FirstRunOverlay({ onClose, onOpenHelp }: FirstRunOverlayProps) {
  return (
    <div className="modal-overlay">
      <div className="modal first-run">
        <h3>Welcome to silman</h3>
        <p>
          The <strong>Game view</strong> is the centrepiece: eval bar + board on the left, the
          engine-free <strong>Explain</strong> panel and the <strong>Moves</strong> panel on the
          right. Use ←/→ to step, ↑/↓ to jump five plies, <strong>f</strong> to flip,{" "}
          <strong>e</strong> to explain.
        </p>
        <p>Everything else lives in the navigation rail on the left edge:</p>
        <ul>
          <li>
            <strong>STUDY</strong> — Database, Game, Opening tree, and Position search.
          </li>
          <li>
            <strong>COACH</strong> — the Explain toggle, Profile (a player&rsquo;s
            strengths/weaknesses report), and Opponent prep.
          </li>
          <li>
            <strong>TRAIN</strong> — Openings SRS (spaced repetition), Tactics drills, and the
            Endgames curriculum.
          </li>
          <li>
            <strong>DATA IN / OUT</strong> — Import PGN / SCID, TWIC ingest, Account syncs, and
            the engine Jobs queue.
          </li>
        </ul>
        <p>
          <strong>Help &amp; tour</strong> at the bottom of the rail opens the full user guide
          any time; <strong>Settings</strong> sits beside it.
        </p>
        <div className="modal-buttons">
          <button onClick={onOpenHelp}>Open the user guide</button>
          <button className="primary" onClick={onClose}>
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}
