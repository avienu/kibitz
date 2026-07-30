/**
 * Opening Lab (run 11): typed IPC wrappers over src-tauri/src/lab.rs plus
 * the pure display logic OpeningLabView uses (unit-tested in
 * openingLab.test.ts). All `invoke` usage for the Lab surface lives here;
 * branch extensions reuse lib/triage's triageExtend / triageExtensionStatus.
 *
 * The verdict paragraph is the product: it is assembled here from the
 * report's honest numbers via the exported templates, so tests can pin
 * the exact wording per case.
 */
import { invoke } from "@tauri-apps/api/core";

/* ---- wire types (src-tauri/src/lab.rs + kibitz-db opening_lab.rs) ---- */

export interface CohortRow {
  color: "white" | "black";
  /** Opening family display name, e.g. "Nimzo-Indian Defense". */
  family: string;
  ecoMin: string;
  ecoMax: string;
  /** The exact ECO codes behind the family (the report's cohort key). */
  ecos: string[];
  games: number;
}

export interface ExitStats {
  leftBook: number;
  stillInBook: number;
  medianExitPly: number | null;
}

export interface AtExitStats {
  evaluated: number;
  equal: number;
  better: number;
  worse: number;
}

export interface ErrorStats {
  analyzedGames: number;
  gamesWithError: number;
  bookPhase: number;
  middlegame: number;
  noErrorFound: number;
  medianErrorPly: number | null;
  middlegameP25Ply: number | null;
  middlegameP75Ply: number | null;
}

export interface StructureStat {
  flag: string;
  games: number;
  scorePct: number;
  /** games × (0.5 − score), clamped at 0 — frequency × deficit. */
  damage: number;
}

export interface LabReply {
  san: string;
  games: number;
  /** True when the position after the reply is still in theory. */
  inBook: boolean;
}

export interface LabMove {
  san: string;
  games: number;
  scorePct: number;
  /** Mean stored eval after the move, user POV cp; null = no evals. */
  avgEvalCp: number | null;
  evalGames: number;
  inBook: boolean;
  inRep: boolean;
  damage: number;
  replies: LabReply[];
}

export interface LabExample {
  gameId: number;
  ply: number;
  white: string;
  black: string;
  date: string;
  san: string;
}

export interface LabNode {
  fen: string;
  ply: number;
  line: string;
  games: number;
  eco: string | null;
  openingName: string | null;
  repSan: string | null;
  hasExtension: boolean;
  damage: number;
  moves: LabMove[];
  examples: LabExample[];
}

export interface HomeworkRow {
  gameId: number;
  ply: number;
  white: string;
  black: string;
  date: string;
  swingCp: number;
  beforeCp: number;
  afterCp: number;
  structures: string[];
}

export interface LabReport {
  player: string;
  color: "white" | "black";
  ecos: string[];
  games: number;
  scorePct: number;
  unanalyzedGames: number;
  exit: ExitStats;
  atExit: AtExitStats;
  errors: ErrorStats;
  structures: StructureStat[];
  nodes: LabNode[];
  homework: HomeworkRow[];
}

export interface FitFlag {
  flag: string;
  scorePct: number | null;
  games: number;
}

export interface LineFit {
  flags: FitFlag[];
  /** False = no cached profile; say "build a profile", never invent. */
  fitAvailable: boolean;
  profilePlayer: string | null;
  profileBuiltAt: string | null;
}

export interface LabReanalyzeEstimate {
  games: number;
  jobs: number;
  totalEstimateMs: number;
  /** Honesty string — show verbatim. */
  estimateBasis: string;
}

export interface LabReanalyzeStarted {
  gamesEnqueued: number;
  jobsEnqueued: number;
  pending: number;
  workerActive: boolean;
}

/* ---- invokes ---- */

export function labCohorts(player: string): Promise<CohortRow[]> {
  return invoke<CohortRow[]>("lab_cohorts", { player });
}

export function labReport(
  player: string,
  color: "white" | "black",
  ecos: string[],
): Promise<LabReport> {
  return invoke<LabReport>("lab_report", { player, color, ecos });
}

export function labLineFit(fen: string, sans: string[]): Promise<LineFit> {
  return invoke<LineFit>("lab_line_fit", { fen, sans });
}

export function labReanalyzeEstimate(
  player: string,
  color: "white" | "black",
  ecos: string[],
): Promise<LabReanalyzeEstimate> {
  return invoke<LabReanalyzeEstimate>("lab_reanalyze_estimate", { player, color, ecos });
}

/** Enqueue the cohort re-analysis AND start the job worker (the click is
 * the explicit engine request). */
export function labReanalyzeStart(
  player: string,
  color: "white" | "black",
  ecos: string[],
): Promise<LabReanalyzeStarted> {
  return invoke<LabReanalyzeStarted>("lab_reanalyze_start", { player, color, ecos });
}

/* ---- pure display helpers ---- */

/** Fullmove number of a 1-based ply (ply 9 = move 5). */
export function moveNo(ply: number): number {
  return Math.ceil(ply / 2);
}

/** "as White · E20–E59 · 412 games" cohort caption. */
export function cohortCaption(c: CohortRow): string {
  const range = c.ecoMin === c.ecoMax ? c.ecoMin : `${c.ecoMin}–${c.ecoMax}`;
  const side = c.color === "white" ? "as White" : "as Black";
  return `${side} · ${range} · ${c.games} game${c.games === 1 ? "" : "s"}`;
}

/** User-POV cp → "+0.2" / "−1.6" pawn display. */
export function formatUserCp(cp: number): string {
  const v = cp / 100;
  const s = Math.abs(v).toFixed(1);
  return v >= 0 ? `+${s}` : `−${s}`;
}

/** Coverage of a branch move: how many of the opponent replies the user
 * actually faced stay inside theory. Null when no replies were observed
 * (the honest "no data" state). */
export function coverage(move: LabMove): { inBook: number; total: number; pct: number } | null {
  const total = move.replies.reduce((n, r) => n + r.games, 0);
  if (total === 0) return null;
  const inBook = move.replies.filter((r) => r.inBook).reduce((n, r) => n + r.games, 0);
  return { inBook, total, pct: Math.round((inBook / total) * 100) };
}

/** Coverage for a CANDIDATE line at a node: only real observed-reply data
 * counts — a candidate the user never played has none. */
export function candidateCoverage(
  node: LabNode,
  firstSan: string,
): { inBook: number; total: number; pct: number } | null {
  const played = node.moves.find((m) => m.san === firstSan);
  return played ? coverage(played) : null;
}

/** FITS-column text for one candidate line. Null = no cached profile
 * (the UI renders the "build a profile" affordance instead). */
export function fitLabel(fit: LineFit | null): string | null {
  if (!fit || !fit.fitAvailable) return null;
  if (fit.flags.length === 0) return "no distinctive structures";
  return fit.flags
    .map((f) =>
      f.scorePct !== null
        ? `${f.flag} ${f.scorePct.toFixed(1)}% (${f.games} game${f.games === 1 ? "" : "s"})`
        : `${f.flag} (no games in profile)`,
    )
    .join(" · ");
}

/** The honest unanalyzed-games banner; null when every game has evals. */
export function unanalyzedNotice(r: LabReport): string | null {
  if (r.unanalyzedGames === 0) return null;
  const n = r.unanalyzedGames;
  return `${n} of ${r.games} games have no engine evals — eval and first-error findings skip them.`;
}

/* ---- the verdict paragraph (the product) ---- */

/** Exported so tests pin the exact wording; `${…}` slots are filled by
 * verdictText. */
export const VERDICT_TEMPLATES = {
  noGames: "No decided games in this cohort yet — import or sync some first.",
  allUnanalyzed: (games: number, exit: string) =>
    `You have ${games} games here and ${exit}. None of them have engine evals, ` +
    `so where the damage happens is honestly unknown — run the re-analysis below to find out.`,
  openingWorse: (worse: number, evaluated: number, exit: string) =>
    `You are often already worse when you leave book: in ${worse} of ${evaluated} ` +
    `evaluated games you were down more than half a pawn ${exit}. The opening itself ` +
    `is costing you here — start with the highest-damage branches below and adopt a ` +
    `sounder book move.`,
  bookPhaseErrors: (bookPhase: number, withError: number, exit: string) =>
    `Your first significant mistakes come in the book phase: ${bookPhase} of ` +
    `${withError} first errors happen at or before the book exit (${exit}). ` +
    `Tightening the branches below should pay off directly.`,
  middlegame: (
    exit: string,
    okPct: number,
    middlegame: number,
    withError: number,
    range: string,
    structure: string,
  ) =>
    `Your games ${exit} with ${okPct}% of evaluated games still equal or better — ` +
    `the opening is not where they die. The damage happens around ${range}: ` +
    `${middlegame} of ${withError} first errors come after book${structure}. ` +
    `That is a middlegame-understanding gap, not a memorization gap — see the ` +
    `structure homework below.`,
  noErrors: (analyzed: number, scorePct: number) =>
    `No significant errors (≥ 1.2 pawns) found in the ${analyzed} analyzed games, ` +
    `and you score ${scorePct}% here. Whatever is going wrong is subtler than a ` +
    `tactical swing — the branch table below still shows where results lag.`,
  noEvidence:
    "Not enough evaluated games to locate the damage yet — run the re-analysis below.",
} as const;

/** Exit phrasing shared by several templates. */
function exitPhrase(r: LabReport): string {
  if (r.exit.medianExitPly !== null) {
    return `leave book around move ${moveNo(r.exit.medianExitPly)}`;
  }
  return "mostly stay in book through the opening window";
}

/** The honest one-paragraph verdict. */
export function verdictText(r: LabReport): string {
  if (r.games === 0) return VERDICT_TEMPLATES.noGames;
  const exit = exitPhrase(r);
  if (r.unanalyzedGames === r.games) {
    return VERDICT_TEMPLATES.allUnanalyzed(r.games, exit);
  }
  const { evaluated, equal, better, worse } = r.atExit;
  if (evaluated > 0 && worse > equal + better) {
    const by =
      r.exit.medianExitPly !== null ? `by move ${moveNo(r.exit.medianExitPly)}` : "at the exit";
    return VERDICT_TEMPLATES.openingWorse(worse, evaluated, by);
  }
  const e = r.errors;
  if (e.gamesWithError > 0 && e.bookPhase > e.middlegame) {
    return VERDICT_TEMPLATES.bookPhaseErrors(e.bookPhase, e.gamesWithError, exit);
  }
  if (e.gamesWithError > 0) {
    const okPct = evaluated > 0 ? Math.round(((equal + better) / evaluated) * 100) : 0;
    const range =
      e.middlegameP25Ply !== null && e.middlegameP75Ply !== null
        ? e.middlegameP25Ply === e.middlegameP75Ply
          ? `move ${moveNo(e.middlegameP25Ply)}`
          : `moves ${moveNo(e.middlegameP25Ply)}–${moveNo(e.middlegameP75Ply)}`
        : "the middlegame";
    const killer = r.structures.find((s) => s.damage > 0);
    const structure = killer ? `, most often in ${killer.flag} positions` : "";
    return VERDICT_TEMPLATES.middlegame(
      exit,
      okPct,
      e.middlegame,
      e.gamesWithError,
      range,
      structure,
    );
  }
  if (e.analyzedGames > 0) {
    return VERDICT_TEMPLATES.noErrors(e.analyzedGames, r.scorePct);
  }
  return VERDICT_TEMPLATES.noEvidence;
}

/* ---- small state selectors for honest UI states ---- */

/** True when the report has evals for nothing — the eval-dependent
 * columns should not render at all. */
export function fullyUnanalyzed(r: LabReport): boolean {
  return r.games > 0 && r.unanalyzedGames === r.games;
}

/** One-line stat chips under the verdict. */
export function statChips(r: LabReport): string[] {
  const chips = [
    `${r.games} game${r.games === 1 ? "" : "s"}`,
    `score ${r.scorePct}%`,
    `${r.exit.leftBook} left book · ${r.exit.stillInBook} stayed in`,
  ];
  if (r.errors.analyzedGames > 0) {
    chips.push(
      `first errors: ${r.errors.bookPhase} book phase · ${r.errors.middlegame} middlegame`,
    );
  }
  if (r.unanalyzedGames > 0) chips.push(`${r.unanalyzedGames} unanalyzed`);
  return chips;
}
