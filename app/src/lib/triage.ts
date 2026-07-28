/**
 * Opening triage (run 10): typed IPC wrappers over src-tauri/src/triage.rs
 * plus the pure display helpers TriageView uses (unit-tested in
 * triage.test.ts). All `invoke` usage for the triage surface lives here.
 */
import { invoke } from "@tauri-apps/api/core";

/* ---- report ---- */

export interface TriageExample {
  gameId: number;
  /** 1-based mainline ply of the triage point in this game (deep-link:
   * showing the position after this ply shows the point). */
  ply: number;
  white: string;
  black: string;
  date: string;
  /** Deviations only: what the user played in this game. */
  playedSan?: string | null;
}

export interface TriageItem {
  /** Position of the point — also what a book extension analyses. */
  fen: string;
  /** Earliest ply the point was reached at. */
  ply: number;
  /** Games that hit this exact point (the ranking key). */
  games: number;
  /** Numbered SAN of the earliest example's path to the point. */
  line: string;
  eco: string | null;
  openingName: string | null;
  /** Deviations: the card's move. */
  expectedSan: string | null;
  /** Deviations: what the user played (earliest example). */
  playedSan: string | null;
  /** Gaps: the opponent's uncovered move. */
  opponentSan: string | null;
  /** True when a completed engine extension is stored for `fen`. */
  hasExtension: boolean;
  examples: TriageExample[];
}

export interface ColorTriage {
  color: "white" | "black";
  /** False = this color has no repertoire cards (nothing to triage). */
  hasCards: boolean;
  gamesScanned: number;
  deviations: TriageItem[];
  gaps: TriageItem[];
  frontiers: TriageItem[];
}

export interface TriageReport {
  player: string;
  white: ColorTriage;
  black: ColorTriage;
}

export function triageReport(player: string): Promise<TriageReport> {
  return invoke<TriageReport>("triage_report", { player });
}

/* ---- book extensions ---- */

export interface CandidateLine {
  /** SAN moves from the analysed position, alternating sides. */
  sans: string[];
  /** Eval from the analysed position's side-to-move POV. */
  scoreCp: number;
  mate?: number | null;
}

export interface BookExtension {
  id: number;
  fen: string;
  requestedAt: string;
  engine: string;
  depth: number;
  multipv: number;
  lines: CandidateLine[];
}

export interface ExtendStarted {
  jobId: number;
  /** False when an existing job for the same position was reused. */
  created: boolean;
  workerActive: boolean;
}

/** Enqueue the 4-line deep analysis AND start the job worker (the click
 * is the explicit engine request). */
export function triageExtend(fen: string): Promise<ExtendStarted> {
  return invoke<ExtendStarted>("triage_extend", { fen });
}

export interface ExtensionStatus {
  extension: BookExtension | null;
  /** "pending" | "running" | "done" | "failed"; null = never requested. */
  jobStatus: string | null;
  /** Queue rows ahead of a pending job (honest wait explanation). */
  jobsAhead: number;
  workerActive: boolean;
}

export function triageExtensionStatus(fen: string): Promise<ExtensionStatus> {
  return invoke<ExtensionStatus>("triage_extension_status", { fen });
}

/* ---- pure display helpers ---- */

/** Side to move of a FEN ("w" | "b"). */
function fenStm(fen: string): "w" | "b" {
  return fen.split(" ")[1] === "b" ? "b" : "w";
}

/**
 * White-POV eval label for a candidate line: "+0.35" / "−1.10" or "#5" /
 * "#−3" (mate FOR White / BY Black). Stored evals are side-to-move POV.
 */
export function evalLabel(line: CandidateLine, fen: string): string {
  const flip = fenStm(fen) === "b" ? -1 : 1;
  const mate = line.mate ?? null;
  if (mate !== null) {
    const m = mate * flip;
    return m >= 0 ? `#${m}` : `#−${-m}`;
  }
  const v = (line.scoreCp * flip) / 100;
  const s = Math.abs(v).toFixed(2);
  return v >= 0 ? `+${s}` : `−${s}`;
}

/**
 * Number a SAN sequence that starts mid-game, using the FEN's move number
 * and side to move: ["Nf3","d6","d4"] from a "w …  0 2" FEN becomes
 * "2. Nf3 d6 3. d4".
 */
export function numberedLine(sans: string[], fen: string): string {
  const fields = fen.split(" ");
  let whiteToMove = fenStm(fen) === "w";
  let moveNo = Number.parseInt(fields[5] ?? "1", 10) || 1;
  const out: string[] = [];
  sans.forEach((san, i) => {
    if (whiteToMove) out.push(`${moveNo}. ${san}`);
    else if (i === 0) out.push(`${moveNo}... ${san}`);
    else out.push(san);
    if (!whiteToMove) moveNo += 1;
    whiteToMove = !whiteToMove;
  });
  return out.join(" ");
}

/** "3 deviations · 1 gap · 2 frontiers" (only non-zero classes named;
 * empty-but-scanned reports say so honestly). */
export function triageSummary(ct: ColorTriage): string {
  const part = (n: number, name: string) => `${n} ${name}${n === 1 ? "" : "s"}`;
  const parts: string[] = [];
  if (ct.deviations.length > 0) parts.push(part(ct.deviations.length, "deviation"));
  if (ct.gaps.length > 0) parts.push(part(ct.gaps.length, "gap"));
  if (ct.frontiers.length > 0) parts.push(part(ct.frontiers.length, "frontier"));
  if (parts.length === 0) {
    return `no triage points in ${ct.gamesScanned} game${ct.gamesScanned === 1 ? "" : "s"}`;
  }
  return parts.join(" · ");
}

/** One-line description of a triage row, per class. */
export function itemCaption(kind: "deviation" | "gap" | "frontier", item: TriageItem): string {
  switch (kind) {
    case "deviation":
      return `book: ${item.expectedSan ?? "?"} — played ${item.playedSan ?? "?"}`;
    case "gap":
      return `opponent played ${item.opponentSan ?? "?"} — no card after it`;
    case "frontier":
      return "your book ends here";
  }
}
