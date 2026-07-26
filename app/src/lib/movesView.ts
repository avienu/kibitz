/**
 * Moves-panel row model (design/handoff-1/README.md §D Moves &
 * annotations): the token stream (or a bare SAN list) laid out as
 * move-pair grid rows — pairs, comment rows, variation rows — with the
 * FRESH vs LEGACY variation classification.
 *
 * Pure logic — no DOM, no Tauri — unit-testable in isolation.
 */

import type { AnalysisRow } from "./analyses";
import { nagSuffix, type AnnItem, type AnnView } from "./tokens";

export interface PairCell {
  /** 1-based mainline ply. */
  ply: number;
  san: string;
  nag: number | null;
  /** Token index for annotation edits; null for non-annotated games. */
  tokenIndex: number | null;
}

export type MovesRow =
  | {
      kind: "pair";
      /** Full-move number. */
      num: number;
      /** True when the white cell is an ellipsis continuation ("…"). */
      whiteEllipsis: boolean;
      white: PairCell | null;
      black: PairCell | null;
    }
  | { kind: "comment"; tokenIndex: number; text: string }
  | {
      kind: "variation";
      varStartIndex: number;
      style: VariationStyle;
      tag: string;
      line: string;
    };

export type VariationStyle = "fresh" | "legacy" | "plain";

/* ------------------------------------------------------------------ */
/* Variation classification (FRESH vs LEGACY vs plain)                 */
/* ------------------------------------------------------------------ */

/** Engine identities present in the game's stored analyses, per kind. */
export interface GameEngines {
  fresh: string[];
  legacy: string[];
}

/** Engine identity strings per kind, from the raw analyses rows. */
export function gameEngines(rows: AnalysisRow[]): GameEngines {
  const fresh = new Set<string>();
  const legacy = new Set<string>();
  for (const r of rows) (r.kind === "fresh" ? fresh : legacy).add(r.engine);
  return { fresh: [...fresh], legacy: [...legacy] };
}

const YEAR = /\b(19[7-9]\d|20[01]\d)\b/;
const DEPTH = /(?:\bdepth\s*|\/)(\d{1,2})\b/i;

/** First word of an engine identity ("Stockfish 18" → "stockfish"). */
function engineStem(engine: string): string {
  return engine.split(/[\s/]+/)[0]?.toLowerCase() ?? "";
}

/**
 * Classify one variation from its embedded comment texts.
 *
 * The token stream does not record variation provenance, so this is a
 * documented heuristic over what IS stored: a variation whose comments
 * name one of the game's legacy engines, or carry a pre-2020 year, is a
 * legacy (imported) engine line; one naming a fresh engine is a fresh
 * engine line; everything else is a plain (human) variation.
 */
export function classifyVariation(
  comments: string[],
  engines: GameEngines,
): { style: VariationStyle; tag: string } {
  const text = comments.join(" ");
  const lower = text.toLowerCase();

  for (const e of engines.legacy) {
    const stem = engineStem(e);
    if (stem && lower.includes(stem)) {
      const year = e.match(YEAR)?.[1] ?? text.match(YEAR)?.[1];
      return { style: "legacy", tag: year ? `LEGACY ${year}` : "LEGACY IMPORT" };
    }
  }
  for (const e of engines.fresh) {
    const stem = engineStem(e);
    if (stem && lower.includes(stem)) {
      const depth = text.match(DEPTH)?.[1];
      return { style: "fresh", tag: depth ? `ENGINE d${depth}` : "ENGINE" };
    }
  }
  const year = text.match(YEAR)?.[1];
  if (year) return { style: "legacy", tag: `LEGACY ${year}` };
  return { style: "plain", tag: "VARIATION" };
}

/** Display order at the same move: fresh before plain before legacy. */
const STYLE_ORDER: Record<VariationStyle, number> = { fresh: 0, plain: 1, legacy: 2 };

/* ------------------------------------------------------------------ */
/* Row building                                                        */
/* ------------------------------------------------------------------ */

/** Full-move number of mainline ply `p` (1-based) given the start FEN. */
function numbering(startFen: string): { startNum: number; whiteFirst: boolean } {
  const parts = startFen.split(" ");
  return {
    startNum: parseInt(parts[5] ?? "1", 10) || 1,
    whiteFirst: (parts[1] ?? "w") === "w",
  };
}

/** Flatten one variation group (varStart..matching varEnd) to a line. */
function variationLine(items: AnnItem[]): { line: string; comments: string[] } {
  const parts: string[] = [];
  const comments: string[] = [];
  for (const it of items) {
    if (it.kind === "move") {
      parts.push(`${it.num ? `${it.num} ` : ""}${it.san}${nagSuffix(it.nag)}`);
    } else if (it.kind === "comment") {
      comments.push(it.text);
      parts.push(`{${it.text}}`);
    }
  }
  return { line: parts.join(" "), comments };
}

/**
 * Lay the annotated token stream out as move-pair grid rows. Comments
 * and variations attach after the move they follow; a black move after
 * an interruption starts a new row with an ellipsis white cell. At one
 * move, fresh variations list before legacy (legacy is retained, never
 * deleted — it just yields the lead).
 */
export function movesRows(view: AnnView, startFen: string, engines: GameEngines): MovesRow[] {
  const rows: MovesRow[] = [];
  const { startNum, whiteFirst } = numbering(startFen);

  let pair: Extract<MovesRow, { kind: "pair" }> | null = null;
  let varBuffer: Extract<MovesRow, { kind: "variation" }>[] = [];

  const flushVars = () => {
    if (varBuffer.length === 0) return;
    varBuffer.sort((a, b) => STYLE_ORDER[a.style] - STYLE_ORDER[b.style]);
    rows.push(...varBuffer);
    varBuffer = [];
  };
  const flushPair = () => {
    // The pair row renders first; its trailing variations follow it.
    if (pair) rows.push(pair);
    pair = null;
    flushVars();
  };

  const items = view.items;
  for (let i = 0; i < items.length; i++) {
    const item = items[i];

    // Top-level variation group: swallow through the matching varEnd.
    if (item.kind === "varStart" && item.depth === 1) {
      let j = i + 1;
      let depth = 1;
      const group: AnnItem[] = [];
      while (j < items.length && depth > 0) {
        const it = items[j];
        if (it.kind === "varStart") depth++;
        else if (it.kind === "varEnd") depth--;
        if (depth > 0) group.push(it);
        j++;
      }
      const { line, comments } = variationLine(group);
      const { style, tag } = classifyVariation(comments, engines);
      varBuffer.push({ kind: "variation", varStartIndex: item.index, style, tag, line });
      i = j - 1;
      continue;
    }
    if (item.kind === "comment") {
      if (item.depth > 0) continue;
      flushPair();
      rows.push({ kind: "comment", tokenIndex: item.index, text: item.text });
      continue;
    }
    if (item.kind !== "move") continue; // stray varStart/varEnd (unreachable at depth 0)

    // Mainline move (depth 0 — buildAnnView only yields depth-0 here
    // outside variation groups, which were swallowed above).
    if (item.mainlinePly === null) continue;
    const ply = item.mainlinePly;
    const cell: PairCell = { ply, san: item.san, nag: item.nag, tokenIndex: item.index };
    const plyOffset = whiteFirst ? ply - 1 : ply; // black-first start shifts parity
    const isWhite = whiteFirst ? ply % 2 === 1 : ply % 2 === 0;
    const num = startNum + Math.floor(plyOffset / 2);

    if (isWhite) {
      flushPair();
      pair = { kind: "pair", num, whiteEllipsis: false, white: cell, black: null };
    } else if (pair && pair.black === null && varBuffer.length === 0) {
      pair.black = cell;
    } else {
      flushPair();
      pair = { kind: "pair", num, whiteEllipsis: true, white: null, black: cell };
    }
  }
  flushPair();
  return rows;
}

/** Pair rows for a plain (non-annotated) game: just the SAN mainline. */
export function movesRowsFromSans(sans: string[], startFen: string): MovesRow[] {
  const rows: MovesRow[] = [];
  const { startNum, whiteFirst } = numbering(startFen);
  let pair: Extract<MovesRow, { kind: "pair" }> | null = null;
  for (let i = 0; i < sans.length; i++) {
    const ply = i + 1;
    const cell: PairCell = { ply, san: sans[i], nag: null, tokenIndex: null };
    const plyOffset = whiteFirst ? ply - 1 : ply;
    const isWhite = whiteFirst ? ply % 2 === 1 : ply % 2 === 0;
    const num = startNum + Math.floor(plyOffset / 2);
    if (isWhite) {
      if (pair) rows.push(pair);
      pair = { kind: "pair", num, whiteEllipsis: false, white: cell, black: null };
    } else if (pair && pair.black === null) {
      pair.black = cell;
      rows.push(pair);
      pair = null;
    } else {
      if (pair) rows.push(pair);
      pair = { kind: "pair", num, whiteEllipsis: true, white: null, black: cell };
      rows.push(pair);
      pair = null;
    }
  }
  if (pair) rows.push(pair);
  return rows;
}

/** NAG glyph colour class: "?" family bad, "!" family accent. */
export function nagTone(nag: number): "bad" | "accent" | "plain" {
  if (nag === 2 || nag === 4 || nag === 6) return "bad"; // ? ?? ?!
  if (nag === 1 || nag === 3 || nag === 5) return "accent"; // ! !! !?
  return "plain";
}
