/**
 * Promotion picker (run-6 item 3): an overlay offering Q/R/B/N whenever a
 * board surface receives a pawn move to the last rank. Click or keys 1–4
 * choose; Esc cancels. Implemented entirely outside Board.tsx — each view
 * guards its own move handler with `usePromotionPicker`.
 */
import { useCallback, useEffect, useState } from "react";
import {
  promoKeyRole,
  promotionPending,
  PROMO_GLYPHS,
  PROMO_ROLES,
  type PromoRole,
} from "./lib/promotion";

interface PickerProps {
  color: "white" | "black";
  onPick: (role: PromoRole) => void;
  onCancel: () => void;
}

const ROLE_KEYS: Record<PromoRole, string> = { queen: "1", rook: "2", bishop: "3", knight: "4" };

export function PromotionPicker({ color, onPick, onCancel }: PickerProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
        return;
      }
      const role = promoKeyRole(e.key);
      if (role) {
        e.preventDefault();
        e.stopPropagation();
        onPick(role);
      }
    };
    // Capture phase so the game view's global key map never sees these.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onPick, onCancel]);

  return (
    <div className="promo-overlay" onClick={onCancel}>
      <div className="promo-card" onClick={(e) => e.stopPropagation()}>
        <div className="promo-title">Promote to</div>
        <div className="promo-roles">
          {PROMO_ROLES.map((role) => (
            <button
              key={role}
              className="promo-btn"
              title={`${role} (${ROLE_KEYS[role]})`}
              onClick={() => onPick(role)}
            >
              <span className="promo-glyph">{PROMO_GLYPHS[color][role]}</span>
              <span className="promo-key">{ROLE_KEYS[role]}</span>
            </button>
          ))}
        </div>
        <div className="promo-hint">1–4 choose · Esc cancels</div>
      </div>
    </div>
  );
}

interface PendingPromo {
  orig: string;
  dest: string;
  color: "white" | "black";
}

/**
 * Guard a board-move handler with the promotion picker. Call
 * `guard(fen, orig, dest)` first in the handler: it returns true (and
 * shows the picker) when the move needs a promotion choice; the original
 * handler is then re-invoked with the chosen role. Render `element`
 * inside a `position: relative` wrapper around the board.
 */
export function usePromotionPicker(
  onComplete: (orig: string, dest: string, role: PromoRole) => void,
) {
  const [pending, setPending] = useState<PendingPromo | null>(null);

  const guard = useCallback((fen: string, orig: string, dest: string): boolean => {
    const color = promotionPending(fen, orig, dest);
    if (!color) return false;
    setPending({ orig, dest, color });
    return true;
  }, []);

  const cancel = useCallback(() => setPending(null), []);
  const pick = useCallback(
    (role: PromoRole) => {
      if (pending) {
        onComplete(pending.orig, pending.dest, role);
        setPending(null);
      }
    },
    [pending, onComplete],
  );

  const element = pending ? (
    <PromotionPicker color={pending.color} onPick={pick} onCancel={cancel} />
  ) : null;

  return { guard, element, active: pending !== null };
}
