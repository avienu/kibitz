/**
 * Nav rail (design/handoff-1 §A): KIBITZ wordmark + db line, the four
 * capability groups and the footer (Settings · Help & tour). Badges are
 * live data or absent — never fake numbers. Collapses to icon-only 56px
 * below 1280px window width (deliverable 2c).
 */
import type { JobsStatus, PlayerProfile, TrainSummary } from "../lib/db";
import { railBadge, RAIL_FOOTER, RAIL_GROUPS, type RailItem, type ViewId } from "../lib/shell";
import type { TacticsState } from "../lib/tactics";

export interface RailData {
  dbGames: number | null;
  explainOn: boolean;
  profile: PlayerProfile | null;
  train: TrainSummary | null;
  tactics: TacticsState | null;
  jobs: JobsStatus | null;
}

interface NavRailProps {
  active: ViewId;
  collapsed: boolean;
  /** e.g. "scid.sqlite · 121,438 games"; null before a db is open. */
  dbLine: string | null;
  data: RailData;
  onNavigate: (id: ViewId) => void;
  /** The Explain item is a toggle, not a route (COACH group). */
  onToggleExplain: () => void;
  onHelp: () => void;
}

export default function NavRail({
  active,
  collapsed,
  dbLine,
  data,
  onNavigate,
  onToggleExplain,
  onHelp,
}: NavRailProps) {
  const item = (it: RailItem) => {
    const badge = railBadge(it.id, data);
    const isActive = it.id === active;
    const onClick =
      it.id === "explain" ? onToggleExplain : it.id === "help" ? onHelp : () => onNavigate(it.id as ViewId);
    return (
      <button
        key={it.id}
        className={`rail-item${isActive ? " active" : ""}`}
        onClick={onClick}
        title={collapsed ? `${it.label}${badge ? ` — ${badge}` : ""}` : undefined}
      >
        {collapsed ? (
          <span className="rail-icon">{it.icon}</span>
        ) : (
          <>
            <span className="rail-label">{it.label}</span>
            {badge && <span className="rail-badge">{badge}</span>}
          </>
        )}
      </button>
    );
  };

  return (
    <nav className={`rail${collapsed ? " collapsed" : ""}`}>
      <div className="rail-header">
        <div className="rail-wordmark">{collapsed ? "S" : "KIBITZ"}</div>
        {!collapsed && <div className="rail-db">{dbLine ?? "no database open"}</div>}
      </div>
      <div className="rail-body">
        {RAIL_GROUPS.map((g) => (
          <div key={g.heading} className="rail-group">
            <div className="rail-heading">{collapsed ? g.heading.slice(0, 2) : g.heading}</div>
            {g.items.map(item)}
          </div>
        ))}
      </div>
      <div className="rail-footer">{RAIL_FOOTER.map(item)}</div>
    </nav>
  );
}
