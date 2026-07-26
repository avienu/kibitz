/**
 * Thin typed wrapper over the Tauri IPC surface of the database browser
 * (src-tauri/src/browse.rs). Keep all `invoke` usage here so the rest of
 * the UI stays pure.
 */
import { invoke } from "@tauri-apps/api/core";
import type { FeatureRecordJson } from "./explainView";
import type { JsonToken } from "./tokens";

const DB_PATH_KEY = "silman.dbPath";

/** Repo-root-relative default; the Rust side resolves it upward from cwd. */
export const DEFAULT_DB_PATH = "testdata/corpus/scid.sqlite";

export function getSavedDbPath(): string {
  return localStorage.getItem(DB_PATH_KEY) ?? DEFAULT_DB_PATH;
}

export function saveDbPath(path: string): void {
  if (path.trim() === "") localStorage.removeItem(DB_PATH_KEY);
  else localStorage.setItem(DB_PATH_KEY, path.trim());
}

export interface DbSummary {
  games: number;
  players: number;
  positions: number;
  sources: number;
  /** Path actually opened (after relative-path resolution). */
  path: string;
}

export interface GameFilter {
  /** Case-insensitive substring match on either player's name. */
  playerSubstring?: string;
  /** ECO prefix match ("C4" matches C40..C49). */
  eco?: string;
  /** Exact result: "1-0" | "0-1" | "1/2-1/2" | "*". */
  result?: string;
}

export interface GameRow {
  id: number;
  white: string;
  black: string;
  whiteElo: number | null;
  blackElo: number | null;
  event: string;
  date: string | null;
  result: string;
  eco: string | null;
  plyCount: number;
}

export interface GameList {
  total: number;
  rows: GameRow[];
}

export interface GameDetail {
  id: number;
  white: string;
  black: string;
  whiteElo: number | null;
  blackElo: number | null;
  event: string;
  site: string;
  round: string | null;
  date: string | null;
  result: string;
  eco: string | null;
  plyCount: number;
  /** null = standard initial position. */
  startFen: string | null;
  /** SAN of every mainline ply, decoded from the movetext blob. */
  sans: string[];
}

export interface TreeRow {
  san: string;
  count: number;
  whiteWins: number;
  draws: number;
  blackWins: number;
  avgElo: number | null;
  perf: number | null;
}

export interface GameAtRow {
  id: number;
  white: string;
  black: string;
  event: string;
  date: string;
  result: string;
  ply: number;
}

export interface GamesAt {
  /** Total games that reached the position (rows is capped server-side). */
  total: number;
  rows: GameAtRow[];
}

export function openDatabase(path: string): Promise<DbSummary> {
  return invoke<DbSummary>("open_database", { path });
}

export function listGames(filter: GameFilter, offset: number, limit: number): Promise<GameList> {
  return invoke<GameList>("list_games", { filter, offset, limit });
}

export function getGame(id: number): Promise<GameDetail> {
  return invoke<GameDetail>("get_game", { id });
}

export function openingTree(fen: string): Promise<TreeRow[]> {
  return invoke<TreeRow[]>("opening_tree", { fen });
}

export function findGamesAt(fen: string): Promise<GamesAt> {
  return invoke<GamesAt>("find_games_at", { fen });
}

/* ---- Phase 2: opponent prep ---- */

export interface MasterGame {
  gameId: number;
  white: string;
  black: string;
  whiteElo: number | null;
  blackElo: number | null;
  event: string;
  date: string;
  result: string;
  /** Ply at which the game reached the weak-line position. */
  ply: number;
}

export interface WeakLine {
  /** Position hash as a decimal string (u64 exceeds JS number range). */
  hash: string;
  /** Earliest ply the opponent reached the position. */
  ply: number;
  /** What the opponent plays there (most frequent first). */
  opponentMoves: string[];
  games: number;
  scorePct: number;
  weakness: number;
  /** True if this spot is also one of the opponent's book-exit points. */
  deviation: boolean;
  masterGames: MasterGame[];
}

export function matchingPlayers(pattern: string): Promise<string[]> {
  return invoke<string[]>("matching_players", { pattern });
}

export function prepView(player: string, color: "white" | "black"): Promise<WeakLine[]> {
  return invoke<WeakLine[]>("prep_view", { player, color });
}

/* ---- Phase 2: annotation editing ---- */

export interface GameTokens {
  /** FEN of the game's start position (standard start included). */
  startFen: string;
  tokens: JsonToken[];
}

export function getGameTokens(gameId: number): Promise<GameTokens> {
  return invoke<GameTokens>("get_game_tokens", { gameId });
}

export function updateGameTokens(gameId: number, tokens: JsonToken[]): Promise<void> {
  return invoke<void>("update_game_tokens", { gameId, tokens });
}

/* ---- Phase 2 stretch: explain position (static analysis, no engine) ---- */

export interface Explanation {
  record: FeatureRecordJson;
  prose: string;
}

export function explainPosition(fen: string): Promise<Explanation> {
  return invoke<Explanation>("explain_position", { fen });
}
