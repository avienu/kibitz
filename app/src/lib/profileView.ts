/**
 * Profile screen view-model (design/handoff-2 §Screen: Profile).
 *
 * Pure logic — claim ids, the serif lede, and per-claim evidence targeting
 * ("every number is a control"). No DOM, no Tauri — unit-testable in
 * isolation. Claim id formats are the round-2 navigation contract used by
 * Home's findings rows: "motif:<Kind>:missed" | "motif:<Kind>:allowed" |
 * "structure:<flag>". Phase/rate claims exist only screen-locally.
 */

import type { MotifRow, PhaseAcpl, PlayerProfile, ProfileExample, StructureRow } from "./db";

/* ---- claims -------------------------------------------------------------- */

export type Claim =
  | { kind: "motif"; motif: string; cell: "missed" | "allowed" }
  | { kind: "structure"; flag: string }
  | { kind: "phase"; phase: "opening" | "middlegame" | "endgame" }
  | { kind: "rate"; rate: "conversion" | "defence" };

/** Parse a navigation claim id; unknown formats return null (no claim). */
export function parseClaim(id: string | null | undefined): Claim | null {
  if (!id) return null;
  const parts = id.split(":");
  if (parts[0] === "motif" && parts.length === 3 && (parts[2] === "missed" || parts[2] === "allowed")) {
    return { kind: "motif", motif: parts[1], cell: parts[2] };
  }
  if (parts[0] === "structure" && parts.length >= 2) {
    return { kind: "structure", flag: parts.slice(1).join(":") };
  }
  if (parts[0] === "phase" && (parts[1] === "opening" || parts[1] === "middlegame" || parts[1] === "endgame")) {
    return { kind: "phase", phase: parts[1] };
  }
  if (parts[0] === "rate" && (parts[1] === "conversion" || parts[1] === "defence")) {
    return { kind: "rate", rate: parts[1] };
  }
  return null;
}

export function claimId(c: Claim): string {
  switch (c.kind) {
    case "motif":
      return `motif:${c.motif}:${c.cell}`;
    case "structure":
      return `structure:${c.flag}`;
    case "phase":
      return `phase:${c.phase}`;
    case "rate":
      return `rate:${c.rate}`;
  }
}

export function sameClaim(a: Claim | null, b: Claim | null): boolean {
  return (a === null && b === null) || (a !== null && b !== null && claimId(a) === claimId(b));
}

/* ---- humanized names ------------------------------------------------------ */

/** AlertKind Debug names → plain-language motif phrases. */
const MOTIF_HUMAN: Record<string, string> = {
  Undefended: "loose pieces left undefended",
  InadequatelyDefended: "under-defended pieces",
  TrappedPiece: "trapped pieces",
  WeakKing: "exposed kings",
};

export function humanMotif(kind: string): string {
  return MOTIF_HUMAN[kind] ?? kind;
}

/** Short form for table rows / facts blocks ("loose piece (LPDO)"). */
const MOTIF_SHORT: Record<string, string> = {
  Undefended: "Loose piece (LPDO)",
  InadequatelyDefended: "Under-defended piece",
  TrappedPiece: "Trapped piece",
  WeakKing: "Exposed king",
};

export function shortMotif(kind: string): string {
  return MOTIF_SHORT[kind] ?? kind;
}

/** "own-isolated-pawn" → "own isolated pawn". */
export function humanFlag(flag: string): string {
  return flag.replace(/-/g, " ");
}

/* ---- the serif lede ------------------------------------------------------- */

/** Motifs ranked by pressure (missed + allowed), worst first. */
export function rankedMotifs(p: PlayerProfile): MotifRow[] {
  return p.motifs
    .filter((m) => m.missed + m.allowed > 0)
    .slice()
    .sort((a, b) => b.missed + b.allowed - (a.missed + a.allowed));
}

/** Structures scoring worst, sample-gated (≥ 2 games), worst first. */
export function weakStructures(p: PlayerProfile): StructureRow[] {
  return p.structures
    .filter((s) => s.games >= 2)
    .slice()
    .sort((a, b) => a.score_pct - b.score_pct);
}

/**
 * The lede names the two dominant findings in prose, from real data only.
 * Falls back honestly when a finding class has no data.
 */
export function profileLede(p: PlayerProfile): string {
  const opening = `Across ${p.games} game${p.games === 1 ? "" : "s"}, ${p.player} scores ${p.score_pct.toFixed(1)}%.`;
  const findings: string[] = [];
  const topMotif = rankedMotifs(p)[0];
  if (topMotif) {
    const parts: string[] = [];
    if (topMotif.allowed > 0) parts.push(`allowed against them ${topMotif.allowed}×`);
    if (topMotif.missed > 0) parts.push(`missed ${topMotif.missed} of ${topMotif.opportunities} chances`);
    findings.push(`${humanMotif(topMotif.kind)} (${parts.join(", ")})`);
  }
  const worstStructure = weakStructures(p).find((s) => s.score_pct < 50);
  if (worstStructure) {
    findings.push(
      `a ${worstStructure.score_pct.toFixed(0)}% score in ${humanFlag(worstStructure.flag)} positions over ${worstStructure.games} games`,
    );
  } else {
    const c = p.conversion;
    if (c.winning_reached > 0 && c.converted_wins < c.winning_reached) {
      findings.push(
        `${c.winning_reached - c.converted_wins} of ${c.winning_reached} winning positions (+2 or better) not converted`,
      );
    }
  }
  if (findings.length === 0) {
    return `${opening} No dominant weakness stands out yet — the motif and structure detectors found too little to accuse.`;
  }
  if (findings.length === 1) {
    return `${opening} One finding dominates: ${findings[0]}.`;
  }
  return `${opening} Two findings dominate: ${findings[0]}, and ${findings[1]}.`;
}

/* ---- evidence targeting --------------------------------------------------- */

export interface ClaimTarget {
  /** Mono count pill, e.g. "12 MISSED" / "22 GAMES". */
  countLabel: string;
  /** Serif what-this-is paragraph, citing real numbers. */
  intro: string;
  /** Supporting {game, ply} examples (capped at 3 per cell upstream). */
  examples: ProfileExample[];
  /** Total occurrences behind the (capped) example list. */
  total: number;
  /** Honest empty-state text when no examples are recorded for the claim. */
  emptyNote?: string;
}

function phaseOf(p: PlayerProfile, phase: "opening" | "middlegame" | "endgame"): PhaseAcpl {
  return phase === "opening" ? p.acpl_opening : phase === "middlegame" ? p.acpl_middlegame : p.acpl_endgame;
}

/** Resolve a claim against the profile into what the evidence pane shows. */
export function claimTarget(p: PlayerProfile, c: Claim): ClaimTarget {
  switch (c.kind) {
    case "motif": {
      const row = p.motifs.find((m) => m.kind === c.motif);
      if (!row) {
        return {
          countLabel: "0 FOUND",
          intro: `No ${humanMotif(c.motif)} were recorded in ${p.player}'s scanned games.`,
          examples: [],
          total: 0,
        };
      }
      if (c.cell === "missed") {
        return {
          countLabel: `${row.missed} MISSED`,
          intro:
            `Positions where a ${humanMotif(row.kind).replace(/s\b/, "")} tactic was available to ${p.player} ` +
            `and not played — ${row.missed} of ${row.opportunities} opportunities went begging. ` +
            `Opening a game lands on the ply where the tactic became available, not the move actually played.`,
          examples: row.example_missed,
          total: row.missed,
        };
      }
      return {
        countLabel: `${row.allowed} ALLOWED`,
        intro:
          `Moves by ${p.player} that newly created ${humanMotif(row.kind)} against them — ${row.allowed} time${row.allowed === 1 ? "" : "s"}. ` +
          `Opening a game lands on the ply where the weakness appeared.`,
        examples: row.example_allowed,
        total: row.allowed,
      };
    }
    case "structure": {
      const row = p.structures.find((s) => s.flag === c.flag);
      if (!row) {
        return {
          countLabel: "0 GAMES",
          intro: `No games with the ${humanFlag(c.flag)} structure were found.`,
          examples: [],
          total: 0,
        };
      }
      return {
        countLabel: `${row.games} GAMES`,
        intro:
          `Games where ${p.player} played with the ${humanFlag(row.flag)} structure — ` +
          `${row.games} game${row.games === 1 ? "" : "s"}, scoring ${row.score_pct.toFixed(1)}% against the 50% baseline tick. ` +
          `Opening one lands on the ply where the structure was assessed.`,
        examples: row.examples,
        total: row.games,
      };
    }
    case "phase": {
      const a = phaseOf(p, c.phase);
      return {
        countLabel: `${a.moves} MOVES`,
        intro:
          `${c.phase[0].toUpperCase()}${c.phase.slice(1)} accuracy: ${a.acpl.toFixed(1)} average centipawns lost ` +
          `over ${a.moves} evaluated moves — ${a.blunders} blunders, ${a.mistakes} mistakes, ${a.inaccuracies} inaccuracies.`,
        examples: [],
        total: a.moves,
        emptyNote:
          "Phase accuracy is aggregated across every evaluated move — no per-game example list is recorded for phase claims yet.",
      };
    }
    case "rate": {
      const conv = p.conversion;
      if (c.rate === "conversion") {
        return {
          countLabel: `${conv.winning_reached} GAMES`,
          intro:
            `Games where ${p.player} reached +2.00 or better: ${conv.winning_reached}, of which ` +
            `${conv.converted_wins} became wins (${conv.winning_reached - conv.converted_wins} did not).`,
          examples: [],
          total: conv.winning_reached,
          emptyNote:
            "Conversion counts come from the eval traces — no per-game example list is recorded for rate claims yet.",
        };
      }
      return {
        countLabel: `${conv.losing_reached} GAMES`,
        intro:
          `Games where ${p.player} slipped to −1.00 or worse: ${conv.losing_reached}, of which ` +
          `${conv.held} were held to a draw or better.`,
        examples: [],
        total: conv.losing_reached,
        emptyNote:
          "Defence counts come from the eval traces — no per-game example list is recorded for rate claims yet.",
      };
    }
  }
}

/* ---- tiles ---------------------------------------------------------------- */

/** "0 blunders · 3 mistakes · 5 inaccuracies / 120 moves" (real counts). */
export function phaseNote(a: PhaseAcpl): string {
  return `${a.blunders} blunder${a.blunders === 1 ? "" : "s"} · ${a.mistakes} mistake${a.mistakes === 1 ? "" : "s"} · ${a.inaccuracies} inacc. / ${a.moves} moves`;
}

/** Percentage string or "—" when the denominator is zero (never fake). */
export function ratePct(num: number, den: number): string {
  return den === 0 ? "—" : `${Math.round((num / den) * 100)}%`;
}

/** The tactics queue can only train motif claims. */
export function trainableMotif(c: Claim | null): string | null {
  return c && c.kind === "motif" ? c.motif : null;
}
