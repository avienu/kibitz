/**
 * Screen header — the round-1 header bar (§B) componentized for round-2
 * screens that own their header content: title, faint subtitle, optional
 * right-aligned actions. Not a new pattern — same .header-bar chrome.
 */
import type { ReactNode } from "react";

export interface ScreenHeaderProps {
  title: string;
  subtitle?: ReactNode;
  /** Right-aligned secondary buttons / segmented controls. */
  actions?: ReactNode;
}

export default function ScreenHeader({ title, subtitle, actions }: ScreenHeaderProps) {
  return (
    <header className="header-bar">
      <div className="header-title-block">
        <div className="header-title-row">
          <span className="header-title">{title}</span>
        </div>
        {subtitle && <div className="header-meta">{subtitle}</div>}
      </div>
      {actions && <div className="header-actions">{actions}</div>}
    </header>
  );
}
