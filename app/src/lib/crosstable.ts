/**
 * Crosstable model (run 10): the games of one event laid out as a
 * players × rounds grid, computed entirely from what the database stores.
 *
 * Round parsing is tolerant by design: "1" and "1.2" bucket into major
 * round 1; "?", "" , missing, or non-numeric tags land in a trailing
 * "unrounded" column — never a crash. Events whose games mostly lack
 * parseable rounds (Swiss dumps, bare TWIC headers) degrade to a scored
 * player list: points, games, performance — the honest subset that is
 * still computable.
 *
 * Pure logic + one thin invoke wrapper; everything else is unit-tested
 * in crosstable.test.ts.
 */
import { invoke } from "@tauri-apps/api/core";

export interface CrosstableGameRow {
  id: number;
  white: string;
  black: string;
  whiteElo: number | null;
  blackElo: number | null;
  /** Raw PGN Round tag ("1", "1.2", "?", or null). */
  round: string | null;
  result: string;
  date: string | null;
}

export interface CrosstableGames {
  /** True game count for the event (rows are capped backend-side). */
  total: number;
  rows: CrosstableGameRow[];
}

export function crosstableGames(event: string): Promise<CrosstableGames> {
  return invoke<CrosstableGames>("crosstable_games", { event });
}

/* ------------------------------------------------------------------ */
/* Round parsing                                                       */
/* ------------------------------------------------------------------ */

/**
 * Major round of a PGN Round tag: "1" → 1, "1.2" → 1 (board/sub-round
 * dropped), "03" → 3. "?", "", "-", missing and non-numeric → null (the
 * unrounded bucket).
 */
export function parseRound(round: string | null | undefined): number | null {
  if (round == null) return null;
  const m = round.trim().match(/^(\d+)(?:\..*)?$/);
  if (!m) return null;
  const n = parseInt(m[1], 10);
  return Number.isFinite(n) ? n : null;
}

/* ------------------------------------------------------------------ */
/* Grid model                                                          */
/* ------------------------------------------------------------------ */

/** One game from a player's perspective, placed in a round column. */
export interface CrosstableCell {
  gameId: number;
  opponent: string;
  /** "1" | "½" | "0" | "*" from this player's perspective. */
  score: "1" | "½" | "0" | "*";
  color: "w" | "b";
}

export interface CrosstableRow {
  name: string;
  /** Highest Elo seen for the player across the event's games. */
  elo: number | null;
  /** Points from decided and drawn games ("*" scores nothing). */
  points: number;
  /** Games counted for points (finished games only). */
  counted: number;
  /** All games including unfinished. */
  games: number;
  /** Linear performance: avg opponent Elo + 800·score − 400, over
   * finished games against rated opponents; null when none. */
  perf: number | null;
  /** cells[i] = games in rounds[i]; the trailing entry (when
   * hasUnrounded) is the unparseable-round bucket. */
  cells: CrosstableCell[][];
}

export interface Crosstable {
  /** "grid" when at least half the games carry a parseable round;
   * otherwise "list" (scored player list — the Swiss degrade). */
  mode: "grid" | "list";
  /** Sorted major rounds present (grid mode; empty for list mode). */
  rounds: number[];
  /** True when a trailing "?" column holds unparseable-round games. */
  hasUnrounded: boolean;
  /** Standings order: points desc, then perf desc, then name. */
  players: CrosstableRow[];
  games: number;
}

function scoreOf(result: string, color: "w" | "b"): "1" | "½" | "0" | "*" {
  if (result === "1/2-1/2") return "½";
  if (result === "1-0") return color === "w" ? "1" : "0";
  if (result === "0-1") return color === "w" ? "0" : "1";
  return "*";
}

const POINTS: Record<string, number> = { "1": 1, "½": 0.5, "0": 0 };

interface Acc {
  name: string;
  elo: number | null;
  points: number;
  counted: number;
  games: number;
  oppElos: number[];
  oppScore: number; // points in games against rated opponents
  byRound: Map<number | null, CrosstableCell[]>;
}

/** Build the crosstable for one event's games. Never throws on ragged
 * data — unparseable rounds bucket, unknown players group under "?". */
export function buildCrosstable(games: readonly CrosstableGameRow[]): Crosstable {
  const players = new Map<string, Acc>();
  const acc = (name: string, elo: number | null): Acc => {
    let a = players.get(name);
    if (!a) {
      a = {
        name,
        elo: null,
        points: 0,
        counted: 0,
        games: 0,
        oppElos: [],
        oppScore: 0,
        byRound: new Map(),
      };
      players.set(name, a);
    }
    if (elo !== null && (a.elo === null || elo > a.elo)) a.elo = elo;
    return a;
  };

  let parseable = 0;
  const roundSet = new Set<number>();
  let hasUnrounded = false;

  for (const g of games) {
    const round = parseRound(g.round);
    if (round !== null) {
      parseable++;
      roundSet.add(round);
    } else {
      hasUnrounded = true;
    }
    for (const color of ["w", "b"] as const) {
      const me = color === "w" ? g.white : g.black;
      const opp = color === "w" ? g.black : g.white;
      const myElo = color === "w" ? g.whiteElo : g.blackElo;
      const oppElo = color === "w" ? g.blackElo : g.whiteElo;
      const a = acc(me, myElo);
      const score = scoreOf(g.result, color);
      a.games++;
      if (score !== "*") {
        a.points += POINTS[score];
        a.counted++;
        if (oppElo !== null) {
          a.oppElos.push(oppElo);
          a.oppScore += POINTS[score];
        }
      }
      const cell: CrosstableCell = { gameId: g.id, opponent: opp, score, color };
      const list = a.byRound.get(round);
      if (list) list.push(cell);
      else a.byRound.set(round, [cell]);
    }
  }

  const rounds = [...roundSet].sort((x, y) => x - y);
  const mode: Crosstable["mode"] =
    games.length > 0 && parseable * 2 >= games.length ? "grid" : "list";

  const rows: CrosstableRow[] = [...players.values()].map((a) => {
    const perf =
      a.oppElos.length > 0
        ? Math.round(
            a.oppElos.reduce((s, e) => s + e, 0) / a.oppElos.length +
              800 * (a.oppScore / a.oppElos.length) -
              400,
          )
        : null;
    const columns: (number | null)[] = hasUnrounded ? [...rounds, null] : rounds;
    return {
      name: a.name,
      elo: a.elo,
      points: a.points,
      counted: a.counted,
      games: a.games,
      perf,
      cells: columns.map((r) => a.byRound.get(r) ?? []),
    };
  });
  rows.sort(
    (x, y) =>
      y.points - x.points || (y.perf ?? -1) - (x.perf ?? -1) || x.name.localeCompare(y.name),
  );

  return { mode, rounds, hasUnrounded, players: rows, games: games.length };
}

/** "2½" / "3" — points formatted the chess way. */
export function formatPoints(points: number): string {
  const whole = Math.floor(points);
  const half = points - whole >= 0.5;
  if (whole === 0 && half) return "½";
  return `${whole}${half ? "½" : ""}`;
}

/** Events named "?" (or blank) have no crosstable identity. */
export function crosstableEligible(event: string | null | undefined): boolean {
  const e = (event ?? "").trim();
  return e !== "" && e !== "?";
}
