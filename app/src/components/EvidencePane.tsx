/**
 * EvidencePane — round-2 shared component 5 of 5 (design/handoff-2
 * §pattern budget). The 420px right aside listing the supporting games
 * behind whatever claim is selected: mono count pill, serif intro,
 * supporting-game rows (title, red mono ply, faint date), footer note and
 * a bottom action-bar slot. Used by Profile, reusable by Prep — the whole
 * "claim → evidence" trust argument lives here.
 *
 * Props contract (stable):
 * - `countLabel`: the mono pill text, e.g. "31 GAMES".
 * - `intro`: serif paragraph explaining what the list shows for the
 *   currently selected claim (re-render with new props to re-target).
 * - `games`: supporting-game rows; `ply` renders red mono ("ply 34").
 * - `onOpenGame(g)`: row click — open the game at the claim's ply.
 * - `footerNote`: faint line under the list.
 * - `actions`: bottom bar slot (primary/secondary buttons).
 * - `title`: header label, default "EVIDENCE".
 * - `empty`: rendered instead of rows when `games` is empty.
 */
import type { ReactNode } from "react";

export interface EvidenceGame {
  /** Database game id (row key + open target). */
  id: number;
  /** "sounix — christoforo · Bergens SK" style row title. */
  title: string;
  /** Ply that produced the claim (red mono). */
  ply: number;
  date?: string | null;
}

export interface EvidencePaneProps {
  countLabel: string;
  intro: ReactNode;
  games: readonly EvidenceGame[];
  onOpenGame?: (game: EvidenceGame) => void;
  footerNote?: ReactNode;
  actions?: ReactNode;
  title?: string;
  empty?: ReactNode;
}

export default function EvidencePane({
  countLabel,
  intro,
  games,
  onOpenGame,
  footerNote,
  actions,
  title = "EVIDENCE",
  empty,
}: EvidencePaneProps) {
  return (
    <aside className="evidence-pane">
      <div className="evidence-pane-head">
        <span className="evidence-pane-title">{title}</span>
        <span className="evidence-pane-pill">{countLabel}</span>
      </div>
      <div className="evidence-pane-body">
        <p className="evidence-pane-intro">{intro}</p>
        {games.length === 0 ? (
          empty && <div className="evidence-pane-empty">{empty}</div>
        ) : (
          <div className="evidence-pane-list">
            {games.map((g) => (
              <button
                key={`${g.id}-${g.ply}`}
                type="button"
                className="evidence-pane-row"
                onClick={onOpenGame ? () => onOpenGame(g) : undefined}
              >
                <span className="evidence-pane-game">{g.title}</span>
                <span className="evidence-pane-ply">ply {g.ply}</span>
                {g.date && <span className="evidence-pane-date">{g.date}</span>}
              </button>
            ))}
          </div>
        )}
        {footerNote && <div className="evidence-pane-footnote">{footerNote}</div>}
      </div>
      {actions && <div className="evidence-pane-actions">{actions}</div>}
    </aside>
  );
}
