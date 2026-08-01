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
  /** Deviations: games that played the DOMINANT off-book move here. */
  playedCount: number;
  /** Deviations: cohort games that actually played the card's move here. */
  cardFollowed: number;
  /** Deviations: the played move dominates the card in the user's own
   * games — this is their real repertoire, not a lapse. */
  realityCheck: boolean;
  /** Reality-check deviations only: what the user actually plays from
   * here (full lines from the standard start through the played move). */
  inferredLines: InferredLine[];
  /** Gaps: the uncovered move was the opponent's FIRST move of the game
   * — a whole-opening hole, not a mid-line gap. */
  wholeOpening: boolean;
  /** True when a completed engine extension is stored for `fen`. */
  hasExtension: boolean;
  examples: TriageExample[];
}

export interface ColorTriage {
  color: "white" | "black";
  /** False = this color has no repertoire cards (its games are skipped;
   * the view offers the inferred-repertoire suggestion flow instead). */
  hasCards: boolean;
  gamesScanned: number;
  /** Games of this color in the walked cohort, whether or not they were
   * triaged — the default-tab signal for card-less colors. */
  gamesSeen: number;
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

/* ---- repertoire inference (the no-repertoire-yet suggestion flow) ---- */

export interface InferredLine {
  /** SAN moves from the standard start (replays legally). */
  sans: string[];
  /** Games whose in-book play followed this whole line. */
  games: number;
  /** The user's points share in those games, percent (one decimal),
   * over the games with a known result. */
  score: number;
  eco: string | null;
  openingName: string | null;
}

export interface InferredRepertoire {
  player: string;
  color: "white" | "black";
  /** Standard-start games of the color walked (0 = the identity has no
   * games of this color at all). */
  gamesScanned: number;
  lines: InferredLine[];
}

/** Infer the lines the user already plays as `color` from their own
 * games (static database walk — no engine). */
export function triageInferRepertoire(
  player: string,
  color: "white" | "black",
): Promise<InferredRepertoire> {
  return invoke<InferredRepertoire>("triage_infer_repertoire", { player, color });
}

/** Rooted inference for a whole-opening hole: what the user already
 * plays from the position after `prefix` (SAN from the standard start).
 * Lines come back full-length from the start; `gamesScanned` counts the
 * cohort games that reached the prefix. Static walk — no engine. */
export function triageInferFrom(
  player: string,
  color: "white" | "black",
  prefix: string[],
): Promise<InferredRepertoire> {
  return invoke<InferredRepertoire>("triage_infer_from", { player, color, prefix });
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

/** The running search, iteration by iteration — what the engine has found
 * so far and how deep it has got. Only ever about the polled position. */
export interface LiveSearch {
  jobId: number;
  fen: string;
  depth: number;
  targetDepth: number;
  nodes: number;
  nps: number;
  lines: CandidateLine[];
}

export interface ExtensionStatus {
  extension: BookExtension | null;
  /** "pending" | "running" | "done" | "failed"; null = never requested. */
  jobStatus: string | null;
  /** Queue rows ahead of a pending job (honest wait explanation). */
  jobsAhead: number;
  workerActive: boolean;
  /** Present only while the engine is searching THIS position. */
  search: LiveSearch | null;
}

export function triageExtensionStatus(fen: string): Promise<ExtensionStatus> {
  return invoke<ExtensionStatus>("triage_extension_status", { fen });
}

/* ---- pure display helpers ---- */

/** "depth 22 of 30 · 41M nodes · 1.8 Mn/s" — the live search caption.
 * Only what the engine actually reported: a zero rate is left out. */
export function searchProgressLabel(s: LiveSearch): string {
  const parts = [`depth ${s.depth} of ${s.targetDepth}`, `${compactCount(s.nodes)} nodes`];
  if (s.nps > 0) parts.push(`${compactCount(s.nps)}/s`);
  return parts.join(" · ");
}

/** 41_200_000 -> "41.2M". Thousands separators are unreadable at a glance
 * when the number changes four times a second. */
export function compactCount(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}G`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return `${n}`;
}

/** How far along a live search is, 0..1 — depth is the only honest axis
 * the engine gives us, and it is wildly non-linear in time, so callers
 * should present it as a position, never as an ETA. */
export function searchProgressFraction(s: LiveSearch): number {
  if (s.targetDepth <= 0) return 0;
  return Math.max(0, Math.min(1, s.depth / s.targetDepth));
}

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

/** Capitalized display name of a color. */
export function colorName(color: "white" | "black"): "White" | "Black" {
  return color === "white" ? "White" : "Black";
}

/**
 * Which color tab the triage screen should open on: a color that has
 * cards (White when both do); when neither has cards, the color with
 * more games in the report's cohort — never a dead tab out of the box
 * (the run-10 SRS default-color rule, applied to triage).
 */
export function defaultTriageColor(r: TriageReport): "white" | "black" {
  if (r.white.hasCards) return "white";
  if (r.black.hasCards) return "black";
  return r.black.gamesSeen > r.white.gamesSeen ? "black" : "white";
}

/** How many opening moves each inferred line shares with the line it
 * continues — 0 when it stands on its own. Inference returns the trunk
 * ("vs 2.Bf4 you play 2...e6", 19 games) ahead of the deeper lines below
 * it, so a continuation is any line an earlier one is a prefix of; the
 * list draws it indented with those shared moves dimmed, and a fanned-out
 * opening reads as one answer plus its detail instead of five near-copies.
 * The longest matching trunk wins — the most specific one. */
export function continuationDepths(lines: InferredLine[]): number[] {
  return lines.map((l, i) => {
    let shared = 0;
    for (let j = 0; j < i; j += 1) {
      const trunk = lines[j].sans;
      if (
        trunk.length > shared &&
        trunk.length < l.sans.length &&
        trunk.every((san, k) => san === l.sans[k])
      ) {
        shared = trunk.length;
      }
    }
    return shared;
  });
}

/** "6 games · 58.3% score · B90 Sicilian Defense" — the caption under an
 * inferred line. Only real data: an unnamed line omits the name part. */
export function inferredLineLabel(l: InferredLine): string {
  const parts = [`${l.games} game${l.games === 1 ? "" : "s"}`, `${l.score}% score`];
  if (l.openingName) {
    parts.push(l.eco ? `${l.eco} ${l.openingName}` : l.openingName);
  }
  return parts.join(" · ");
}

/* ---- declared-vs-played helpers (2026-07-30 field report) ---- */

/** Deviations the reality check flagged: the user's play IS their
 * repertoire — the panel confronts it instead of a scolding row. */
export function realityDeviations(ct: ColorTriage): TriageItem[] {
  return ct.deviations.filter((d) => d.realityCheck);
}

/** Gaps at the opponent's first move: whole-opening holes, one row per
 * opponent move family (positions already collapse them). */
export function wholeOpeningGaps(ct: ColorTriage): TriageItem[] {
  return ct.gaps.filter((g) => g.wholeOpening);
}

/** Mid-line gaps inside a followed book line — the per-move holes the
 * triage rows were built for. */
export function inBookGaps(ct: ColorTriage): TriageItem[] {
  return ct.gaps.filter((g) => !g.wholeOpening);
}

/** Raw SAN tokens of a numbered-SAN line ("1. e4 c5" → ["e4","c5"]).
 * SAN never starts with a digit, so dropping digit-led tokens is exact. */
export function lineSans(line: string): string[] {
  return line.split(/\s+/).filter((t) => t !== "" && !/^\d/.test(t));
}

/** Number a single move played FROM `fen` ("1. e4" / "1... e5" style). */
export function numberedSan(fen: string, san: string): string {
  const moveNo = Number.parseInt(fen.split(" ")[5] ?? "1", 10) || 1;
  return fenStm(fen) === "w" ? `${moveNo}. ${san}` : `${moveNo}... ${san}`;
}

/** Number the user's last carded move of a frontier line. `item.fen` is
 * the position AFTER it, so the side to move there is the OPPONENT, and
 * the move number has already advanced when that side is White. */
export function lastBookMoveLabel(item: TriageItem): string {
  const san = lineSans(item.line).at(-1);
  if (!san) return "the start";
  const fields = item.fen.split(" ");
  const moveNo = Number.parseInt(fields[5] ?? "1", 10) || 1;
  return fields[1] === "b" ? `${moveNo}. ${san}` : `${moveNo - 1}... ${san}`;
}

/** Number the opponent move a gap records (`fen` is the position AFTER
 * it, user to move): after 1.d4 → "1. d4"; after 1.e4 c5 → "1... c5". */
export function opponentMoveLabel(item: TriageItem): string {
  const san = item.opponentSan ?? "?";
  const fields = item.fen.split(" ");
  const moveNo = Number.parseInt(fields[5] ?? "1", 10) || 1;
  return fields[1] === "b" ? `${moveNo}. ${san}` : `${moveNo - 1}... ${san}`;
}

/** "No repertoire vs 1. d4 (63 games)" — the whole-opening-hole row. */
export function wholeGapLabel(item: TriageItem): string {
  return `No repertoire vs ${opponentMoveLabel(item)} (${item.games} game${
    item.games === 1 ? "" : "s"
  })`;
}

/** The reality panel's honest headline: cards vs actual play, counted. */
export function realityHeadline(item: TriageItem): string {
  const total = item.playedCount + item.cardFollowed;
  return (
    `Your cards say ${numberedSan(item.fen, item.expectedSan ?? "?")} — but you've played ` +
    `${numberedSan(item.fen, item.playedSan ?? "?")} in ${item.playedCount} of ${total} game` +
    `${total === 1 ? "" : "s"}. That looks like your real repertoire.`
  );
}

/** The full line (SAN from the standard start) that adopts `san` as the
 * user's answer at a triage item's position: the item's own path plus
 * the move. Works for gaps (path ends with the opponent's move) and for
 * deviations (path ends before the user's move) alike — `item.fen` is
 * the position the user moves from in both. */
export function answerLineSans(item: TriageItem, san: string): string[] {
  return [...lineSans(item.line), san];
}

/** Confirm copy for a board-played answer — never silently write. */
export function answerConfirmCopy(item: TriageItem, san: string): string {
  const target = item.opponentSan ? ` to ${opponentMoveLabel(item)}` : "";
  return `Set ${numberedSan(item.fen, san)} as your repertoire answer${target}?`;
}

/** "your play disagrees with your cards at 1 position · 2 whole-opening
 * holes · 3 in-book gaps · 1 frontier" — only non-zero classes named;
 * empty-but-scanned reports say so honestly. A card-less color says WHY
 * it was skipped instead of the misleading "no triage points in 0
 * games" (2026-07-30 field report, both rounds). */
export function triageSummary(ct: ColorTriage): string {
  if (!ct.hasCards) {
    const c = colorName(ct.color);
    return `${c} games are skipped until a ${c} repertoire exists — adopt one below.`;
  }
  const part = (n: number, name: string) => `${n} ${name}${n === 1 ? "" : "s"}`;
  const parts: string[] = [];
  const dev = ct.deviations.length;
  if (dev > 0) {
    parts.push(`your play disagrees with your cards at ${dev} position${dev === 1 ? "" : "s"}`);
  }
  const holes = wholeOpeningGaps(ct).length;
  if (holes > 0) parts.push(part(holes, "whole-opening hole"));
  const gaps = inBookGaps(ct).length;
  if (gaps > 0) parts.push(part(gaps, "in-book gap"));
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
      return `your book ends after ${lastBookMoveLabel(item)} — no card for anything ${
        fenStm(item.fen) === "w" ? "White" : "Black"
      } plays next`;
  }
}
