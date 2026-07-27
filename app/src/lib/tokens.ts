/**
 * Annotation token view model + pure transforms (Phase 2 annotation UI).
 *
 * Mirrors the wire format of the `get_game_tokens` / `update_game_tokens`
 * commands (src-tauri/src/tokens.rs), which itself mirrors
 * kibitz_db::movebin::Token. Replay semantics are the movebin ones: a
 * variation branches from the position BEFORE the last move at the current
 * nesting level, so consecutive variations all replace the same move.
 *
 * No DOM, no Tauri — unit-testable in isolation.
 */
import { Chess, type Position } from "chessops/chess";
import { makeFen, parseFen } from "chessops/fen";
import { parseSan } from "chessops/san";
import { opposite } from "chessops/util";

export type JsonToken =
  | { t: "move"; san: string }
  | { t: "nag"; value: number }
  | { t: "comment"; text: string }
  | { t: "varStart" }
  | { t: "varEnd" }
  | { t: "null" };

/** Move-suffix NAGs, in the UI's cycle order: none/!/?/!!/??/!?/?!. */
export const NAG_SUFFIX: Record<number, string> = {
  1: "!",
  2: "?",
  3: "!!",
  4: "??",
  5: "!?",
  6: "?!",
};
const NAG_CYCLE = [1, 2, 3, 4, 5, 6];

export function nagSuffix(nag: number | null): string {
  if (nag === null) return "";
  return NAG_SUFFIX[nag] ?? ` $${nag}`;
}

export interface MoveItem {
  kind: "move";
  /** Index of this token in the token list. */
  index: number;
  /** SAN as stored; "--" for null moves. */
  san: string;
  /** Move-number prefix: "1." | "1..." | "" (continuation). */
  num: string;
  /** Variation nesting depth; 0 = mainline. */
  depth: number;
  /** 1-based mainline ply when depth is 0, else null. */
  mainlinePly: number | null;
  /** First NAG attached to this move, if any. */
  nag: number | null;
  /** FEN after the move (variation moves included). */
  fenAfter: string;
}

export interface CommentItem {
  kind: "comment";
  index: number;
  text: string;
  depth: number;
}

export interface ParenItem {
  kind: "varStart" | "varEnd";
  index: number;
  /** Depth of the variation being opened/closed (1-based). */
  depth: number;
}

export type AnnItem = MoveItem | CommentItem | ParenItem;

export interface AnnView {
  items: AnnItem[];
  /** Mainline SAN (nulls as "--"); must match the game model's sans. */
  mainlineSans: string[];
  error: string | null;
}

interface Level {
  pos: Position;
  beforeLast: Position | null;
  /** Whether the next black move needs its "N..." number prefix. */
  needNumber: boolean;
}

/** Side-to-move flip (chessops has no null move); null when not legal. */
function playNull(pos: Position): Position | null {
  const setup = pos.toSetup();
  setup.turn = opposite(setup.turn);
  setup.epSquare = undefined;
  const r = Chess.fromSetup(setup);
  return r.isOk ? r.unwrap() : null;
}

/** Build the renderable item list by replaying the token stream. */
export function buildAnnView(startFen: string, tokens: JsonToken[]): AnnView {
  const items: AnnItem[] = [];
  const mainlineSans: string[] = [];
  const fail = (error: string): AnnView => ({ items, mainlineSans, error });

  const setup = parseFen(startFen);
  if (setup.isErr) return fail(`Bad start FEN: ${setup.error.message}`);
  const p0 = Chess.fromSetup(setup.unwrap());
  if (p0.isErr) return fail(`Illegal start position: ${p0.error.message}`);

  let level: Level = { pos: p0.unwrap(), beforeLast: null, needNumber: true };
  const stack: Level[] = [];
  let mainlinePly = 0;

  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i];
    const depth = stack.length;
    switch (tok.t) {
      case "move":
      case "null": {
        const num =
          level.pos.turn === "white"
            ? `${level.pos.fullmoves}.`
            : level.needNumber
              ? `${level.pos.fullmoves}...`
              : "";
        let san: string;
        if (tok.t === "move") {
          const move = parseSan(level.pos, tok.san);
          if (!move) return fail(`Illegal or unparseable SAN "${tok.san}" (token ${i}).`);
          san = tok.san;
          level.beforeLast = level.pos.clone();
          level.pos.play(move);
        } else {
          const next = playNull(level.pos);
          if (!next) return fail(`Null move is not representable here (token ${i}).`);
          san = "--";
          level.beforeLast = level.pos;
          level.pos = next;
        }
        let ply: number | null = null;
        if (depth === 0) {
          ply = ++mainlinePly;
          mainlineSans.push(san);
        }
        items.push({
          kind: "move",
          index: i,
          san,
          num,
          depth,
          mainlinePly: ply,
          nag: null,
          fenAfter: makeFen(level.pos.toSetup()),
        });
        level.needNumber = false;
        break;
      }
      case "nag": {
        for (let j = items.length - 1; j >= 0; j--) {
          const it = items[j];
          if (it.kind === "move") {
            if (it.nag === null) it.nag = tok.value;
            break;
          }
        }
        break;
      }
      case "comment":
        items.push({ kind: "comment", index: i, text: tok.text, depth });
        level.needNumber = true;
        break;
      case "varStart": {
        if (!level.beforeLast) return fail(`Variation before any move (token ${i}).`);
        items.push({ kind: "varStart", index: i, depth: depth + 1 });
        stack.push(level);
        level = { pos: level.beforeLast.clone(), beforeLast: null, needNumber: true };
        break;
      }
      case "varEnd": {
        const parent = stack.pop();
        if (!parent) return fail(`varEnd without varStart (token ${i}).`);
        items.push({ kind: "varEnd", index: i, depth: stack.length + 1 });
        level = parent;
        level.needNumber = true;
        break;
      }
    }
  }
  return { items, mainlineSans, error: stack.length > 0 ? "Unclosed variation." : null };
}

const isAnnotation = (t: JsonToken) => t.t === "nag" || t.t === "comment";

/** Token index of mainline move #ply (1-based), or -1. */
export function mainlineMoveTokenIndex(tokens: JsonToken[], ply: number): number {
  let depth = 0;
  let count = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.t === "varStart") depth++;
    else if (t.t === "varEnd") depth--;
    else if ((t.t === "move" || t.t === "null") && depth === 0 && ++count === ply) return i;
  }
  return -1;
}

/** Index just past the varEnd matching the varStart at `i`. */
function skipVarGroup(tokens: JsonToken[], i: number): number {
  let depth = 0;
  for (let j = i; j < tokens.length; j++) {
    if (tokens[j].t === "varStart") depth++;
    else if (tokens[j].t === "varEnd" && --depth === 0) return j + 1;
  }
  return tokens.length;
}

/**
 * Insertion point for a NEW variation of the move at `moveIndex`: after
 * the move's NAGs, comments, and existing variation groups, keeping the
 * canonical PGN order (move, annotations, variations, next move).
 */
function attachEnd(tokens: JsonToken[], moveIndex: number): number {
  let j = moveIndex + 1;
  while (j < tokens.length) {
    const t = tokens[j];
    if (isAnnotation(t)) j++;
    else if (t.t === "varStart") j = skipVarGroup(tokens, j);
    else break;
  }
  return j;
}

/**
 * Insert `sans` as a variation replacing mainline move #`mainlinePly`
 * (1-based): VarStart + moves + VarEnd after that move's attachments.
 */
export function insertVariation(
  tokens: JsonToken[],
  mainlinePly: number,
  sans: string[],
): JsonToken[] {
  const i = mainlineMoveTokenIndex(tokens, mainlinePly);
  if (i < 0 || sans.length === 0) return tokens;
  const j = attachEnd(tokens, i);
  const variation: JsonToken[] = [
    { t: "varStart" },
    ...sans.map((san): JsonToken => ({ t: "move", san })),
    { t: "varEnd" },
  ];
  return [...tokens.slice(0, j), ...variation, ...tokens.slice(j)];
}

/** Remove the variation whose varStart token sits at `varStartIndex`. */
export function deleteVariation(tokens: JsonToken[], varStartIndex: number): JsonToken[] {
  if (tokens[varStartIndex]?.t !== "varStart") return tokens;
  const end = skipVarGroup(tokens, varStartIndex);
  return [...tokens.slice(0, varStartIndex), ...tokens.slice(end)];
}

/** The comment attached to the move at `moveIndex` (after its NAGs). */
export function commentAfter(
  tokens: JsonToken[],
  moveIndex: number,
): { index: number; text: string } | null {
  let j = moveIndex + 1;
  while (tokens[j]?.t === "nag") j++;
  const t = tokens[j];
  return t?.t === "comment" ? { index: j, text: t.text } : null;
}

/**
 * Set/replace the comment on the move at `moveIndex`; empty text deletes.
 * The comment slot is right after the move's NAGs (canonical PGN order).
 */
export function setComment(tokens: JsonToken[], moveIndex: number, text: string): JsonToken[] {
  const trimmed = text.trim();
  const existing = commentAfter(tokens, moveIndex);
  if (existing) {
    if (!trimmed) return [...tokens.slice(0, existing.index), ...tokens.slice(existing.index + 1)];
    return tokens.map((t, i): JsonToken => (i === existing.index ? { t: "comment", text: trimmed } : t));
  }
  if (!trimmed) return tokens;
  let j = moveIndex + 1;
  while (tokens[j]?.t === "nag") j++;
  return [...tokens.slice(0, j), { t: "comment", text: trimmed }, ...tokens.slice(j)];
}

/** Delete the comment token at `commentIndex`. */
export function deleteComment(tokens: JsonToken[], commentIndex: number): JsonToken[] {
  if (tokens[commentIndex]?.t !== "comment") return tokens;
  return [...tokens.slice(0, commentIndex), ...tokens.slice(commentIndex + 1)];
}

/**
 * Set (or clear, with null) the move-suffix NAG of the move at
 * `moveIndex` — the NAG-picker popover's direct-selection path.
 */
export function setNag(tokens: JsonToken[], moveIndex: number, value: number | null): JsonToken[] {
  const j = moveIndex + 1;
  const has = tokens[j]?.t === "nag";
  if (value === null) {
    return has ? [...tokens.slice(0, j), ...tokens.slice(j + 1)] : tokens;
  }
  if (has) return tokens.map((t, i): JsonToken => (i === j ? { t: "nag", value } : t));
  return [...tokens.slice(0, j), { t: "nag", value }, ...tokens.slice(j)];
}

/**
 * Cycle the move-suffix NAG of the move at `moveIndex` through
 * none → ! → ? → !! → ?? → !? → ?! → none. A NAG outside the cycle is
 * cleared (treated as the last stop).
 */
export function cycleNag(tokens: JsonToken[], moveIndex: number): JsonToken[] {
  const j = moveIndex + 1;
  const t = tokens[j];
  if (t?.t === "nag") {
    const at = NAG_CYCLE.indexOf(t.value);
    if (at === -1 || at === NAG_CYCLE.length - 1) {
      return [...tokens.slice(0, j), ...tokens.slice(j + 1)];
    }
    const next = NAG_CYCLE[at + 1];
    return tokens.map((tok, i): JsonToken => (i === j ? { t: "nag", value: next } : tok));
  }
  return [...tokens.slice(0, j), { t: "nag", value: NAG_CYCLE[0] }, ...tokens.slice(j)];
}
