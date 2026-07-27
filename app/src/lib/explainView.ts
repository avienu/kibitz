/**
 * View helpers for the explain-position panel: turn a FeatureRecord (spec
 * JSON shape, snake_case fields — docs/KIBITZ_ENGINE_SPEC.md) into the
 * shared evidence-overlay input (lib/evidence.ts). Pure; unit-testable.
 */

import type { Evidence, EvidenceArrow } from "./evidence";

/** Minimal typing of the FeatureRecord JSON the UI consumes. */
export interface AlertJson {
  kind: string;
  side: string;
  target?: string;
  attackers?: string[];
  defenders?: string[];
  severity: string;
  detail?: string;
}

export interface ImbalanceJson {
  kind: string;
  favors: string;
  magnitude: string;
  evidence: Record<string, unknown>;
  plans?: { hint: string; squares?: string[] }[];
}

export interface FeatureRecordJson {
  schema_version: number;
  fen: string;
  side_to_move: string;
  phase: string;
  wsui: { alerts: AlertJson[]; screen_fired: boolean };
  imbalances: ImbalanceJson[];
}

/** Legacy import path for the auto-shape type (now lives in evidence.ts). */
export type { BoardShape } from "./evidence";

const SQUARE = /^[a-h][1-8]$/;

/**
 * Map a FeatureRecord onto the evidence-overlay language: alert targets ring,
 * attackers wedge + arrow into the target, defenders wedge (no arrow),
 * imbalance-evidence squares wash, plan squares key-wedge. Marks stack — a
 * square may carry several roles (the overlay module paint-orders them).
 */
export function evidenceFromRecord(record: FeatureRecordJson): Evidence {
  const alerts: string[] = [];
  const attackers: string[] = [];
  const defenders: string[] = [];
  const imbalance: string[] = [];
  const key: string[] = [];
  const arrows: EvidenceArrow[] = [];

  for (const alert of record.wsui?.alerts ?? []) {
    const target = alert.target && SQUARE.test(alert.target) ? alert.target : undefined;
    if (target) alerts.push(target);
    for (const sq of alert.attackers ?? []) {
      if (!SQUARE.test(sq)) continue;
      attackers.push(sq);
      // Arrows always point attacker → target, never the reverse.
      if (target) arrows.push({ from: sq, to: target, kind: "attacker" });
    }
    for (const sq of alert.defenders ?? []) {
      if (SQUARE.test(sq)) defenders.push(sq);
    }
  }

  for (const imb of record.imbalances ?? []) {
    for (const value of Object.values(imb.evidence ?? {})) {
      if (!Array.isArray(value)) continue;
      for (const sq of value) {
        if (typeof sq === "string" && SQUARE.test(sq)) imbalance.push(sq);
      }
    }
    for (const plan of imb.plans ?? []) {
      for (const sq of plan.squares ?? []) {
        if (SQUARE.test(sq)) key.push(sq);
      }
    }
  }

  return { alerts, attackers, defenders, imbalance, key, arrows };
}
