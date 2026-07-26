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
 * the main tabs and where Help lives. Shown on first launch only; the
 * seen flag persists in localStorage.
 */
export default function FirstRunOverlay({ onClose, onOpenHelp }: FirstRunOverlayProps) {
  return (
    <div className="modal-overlay">
      <div className="modal first-run">
        <h3>Welcome to silman</h3>
        <p>
          The <strong>left column</strong> is always the board, with move navigation, the
          on-demand <strong>Engine</strong> panel, and the instant, engine-free{" "}
          <strong>Explain</strong> panel.
        </p>
        <p>The tabs at the top of the right column switch what you work with:</p>
        <ul>
          <li>
            <strong>Load PGN</strong> — paste or open a PGN file to review a game.
          </li>
          <li>
            <strong>Database</strong> — open a game database: browse and filter games, opening
            tree, annotate, run engine jobs, export PGN.
          </li>
          <li>
            <strong>Opponent Prep</strong> — rank an opponent&rsquo;s weakest opening spots and
            study master games from them.
          </li>
          <li>
            <strong>Player Profile</strong> — a full strengths/weaknesses report for any player
            in the database.
          </li>
          <li>
            <strong>Train</strong> — spaced-repetition review of your opening repertoires.
          </li>
          <li>
            <strong>Tactics</strong> — puzzle drills: rated, by motif, weakness-weighted,
            Woodpecker cycles, speed.
          </li>
          <li>
            <strong>Endgames</strong> — a tiered curriculum of theoretical endgames, played out
            to the end.
          </li>
        </ul>
        <p>
          The <strong>Help</strong> button at the right end of the tab bar opens the full user
          guide any time.
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
