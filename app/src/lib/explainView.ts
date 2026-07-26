/**
 * View helpers for the explain-position panel: turn a FeatureRecord (spec
 * JSON shape, snake_case fields — docs/SILMAN_ENGINE_SPEC.md) into board
 * overlay shapes. Pure; unit-testable.
 */

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

export interface BoardShape {
  orig: string;
  brush: string;
}

const SQUARE = /^[a-h][1-8]$/;

/**
 * Evidence overlay: alert target squares red, attacker squares orange,
 * imbalance-evidence squares green. A square keeps its most alarming color
 * (red > orange > green).
 */
export function shapesFromRecord(record: FeatureRecordJson): BoardShape[] {
  const targets = new Set<string>();
  const attackers = new Set<string>();
  const evidence = new Set<string>();

  for (const alert of record.wsui?.alerts ?? []) {
    if (alert.target && SQUARE.test(alert.target)) targets.add(alert.target);
    for (const sq of alert.attackers ?? []) {
      if (SQUARE.test(sq)) attackers.add(sq);
    }
  }
  for (const imbalance of record.imbalances ?? []) {
    for (const value of Object.values(imbalance.evidence ?? {})) {
      if (!Array.isArray(value)) continue;
      for (const sq of value) {
        if (typeof sq === "string" && SQUARE.test(sq)) evidence.add(sq);
      }
    }
  }

  const shapes: BoardShape[] = [];
  for (const sq of targets) shapes.push({ orig: sq, brush: "red" });
  for (const sq of attackers) {
    if (!targets.has(sq)) shapes.push({ orig: sq, brush: "orange" });
  }
  for (const sq of evidence) {
    if (!targets.has(sq) && !attackers.has(sq)) shapes.push({ orig: sq, brush: "green" });
  }
  return shapes;
}
