/**
 * Thin typed wrapper over the Tauri IPC surface of the database browser
 * (src-tauri/src/browse.rs). Keep all `invoke` usage here so the rest of
 * the UI stays pure.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AnalysisRow } from "./analyses";
import type { FeatureRecordJson } from "./explainView";
import type { ExplanationJson } from "./gameView";
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
  /** Source name (e.g. "TWIC 1594") — the SOURCE column tag. */
  source: string;
  /** Tag colour driver: "personal" | "twic" | "online" | "other". */
  sourceKind: string;
  /** True when duplicate copies are linked to this game (⑂ flag). */
  dup: boolean;
  /** "fresh" | "legacy" | null — round-1 display rule (fresh supersedes). */
  analysisKind: "fresh" | "legacy" | null;
  /** Max fresh depth; null for legacy/none. */
  analysisDepth: number | null;
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
  /** Resolved opening name for `eco` (bundled CC0 dataset); null when the
   * game has no ECO or the code is unknown. */
  openingName: string | null;
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
  /** Measured query time in ms — the results pill is a product claim,
   * never estimated. */
  elapsedMs: number;
}

/** BREAKING (round 2): opening_tree now returns rows plus measured timing. */
export interface OpeningTree {
  rows: TreeRow[];
  /** Measured query time in ms. */
  elapsedMs: number;
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

export function openingTree(fen: string): Promise<OpeningTree> {
  return invoke<OpeningTree>("opening_tree", { fen });
}

/** ECO code → canonical opening name (null when unknown). */
export function ecoNames(codes: string[]): Promise<Record<string, string | null>> {
  return invoke<Record<string, string | null>>("eco_names", { codes });
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
  /** ECO code when the spot is a book position (deviations have none). */
  eco: string | null;
  /** Opening name for `eco` from the bundled CC0 dataset. */
  openingName: string | null;
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

/* ---- Round 2: prep fingerprint (workflow step 2) ---- */

export interface FingerprintRow {
  eco: string;
  /** Resolved opening name (bundled CC0 dataset); null for unknown / "?". */
  name: string | null;
  games: number;
  /** Share of the opponent's games as this color, percent (one decimal). */
  sharePct: number;
  scorePct: number;
}

export interface BookExit {
  /** Position hash (decimal string) the exit was played from. */
  hash: string;
  eco: string | null;
  openingName: string | null;
  /** The move that left book. */
  san: string;
  /** Earliest 0-based ply observed for this exit. */
  ply: number;
  count: number;
  scorePct: number;
}

export interface PrepFingerprint {
  games: number;
  scorePct: number;
  rows: FingerprintRow[];
  bookExits: BookExit[];
}

export function prepFingerprint(
  player: string,
  color: "white" | "black",
): Promise<PrepFingerprint> {
  return invoke<PrepFingerprint>("prep_fingerprint", { player, color });
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
  /** The game-view contract (schema v3, snake_case): tag, eval readout,
   * dual-voice headline and blocks with per-block evidence. */
  explanation: ExplanationJson;
}

/** Narration voice (run-5 item 3): coach (default) or neutral. */
export type NarrationVoice = "coach" | "neutral";

const VOICE_KEY = "silman.narrationVoice";

/** Locally saved voice, used before/without an open database. */
export function getSavedVoice(): NarrationVoice {
  return localStorage.getItem(VOICE_KEY) === "neutral" ? "neutral" : "coach";
}

export function saveVoice(voice: NarrationVoice): void {
  localStorage.setItem(VOICE_KEY, voice);
}

/** The voice stored in the open database's settings (meta table). */
export function getNarrationVoice(): Promise<NarrationVoice> {
  return invoke<NarrationVoice>("get_narration_voice");
}

/** Persist the voice in the open database; narrations regenerate on the
 * next annotate / job fold-back pass. */
export function setNarrationVoice(voice: NarrationVoice): Promise<void> {
  return invoke<void>("set_narration_voice", { voice });
}

export function explainPosition(fen: string, voice?: NarrationVoice): Promise<Explanation> {
  return invoke<Explanation>("explain_position", { fen, voice });
}

/* ---- Run 4: analyses, annotate/re-analyze/jobs, export, profile ---- */

/** All stored evals for one game (fresh rows first per ply). */
export function gameAnalyses(gameId: number): Promise<AnalysisRow[]> {
  return invoke<AnalysisRow[]>("game_analyses", { gameId });
}

export interface AnnotateSummary {
  positionsAnalyzed: number;
  screensFired: number;
  jobsEnqueued: number;
  commentsAdded: number;
}

/** Static Silman annotation pass over one game (enqueues confirm jobs). */
export function annotateGame(gameId: number): Promise<AnnotateSummary> {
  return invoke<AnnotateSummary>("annotate_game", { gameId });
}

/** Enqueue a bounded eval per mainline position; returns the job count. */
export function reanalyzeGame(gameId: number): Promise<number> {
  return invoke<number>("reanalyze_game", { gameId });
}

/** Start the background job worker (the user-initiated engine entry point). */
export function runJobs(): Promise<void> {
  return invoke<void>("run_jobs");
}

export interface JobsStatus {
  pending: number;
  running: number;
  done: number;
  failed: number;
  /** True while the run_jobs worker thread is alive. */
  workerActive: boolean;
  /** Engine identity of the most recently completed job, if any. */
  engine: string | null;
}

export function jobsStatus(): Promise<JobsStatus> {
  return invoke<JobsStatus>("jobs_status");
}

/** Render one stored game (with annotations) as PGN text. */
export function exportGamePgn(gameId: number): Promise<string> {
  return invoke<string>("export_game_pgn", { gameId });
}

/* Player profile (goal 4). Field names are snake_case: the payload is the
 * silman-profile PlayerProfile record serialized as-is. */

export interface PhaseAcpl {
  moves: number;
  acpl: number;
  blunders: number;
  mistakes: number;
  inaccuracies: number;
}

/** One piece of drill-down evidence: the game and the 1-based mainline
 * ply that produced the claim (ply 0 = game-level claim, opens at start). */
export interface ProfileExample {
  game: number;
  ply: number;
}

export interface MotifRow {
  kind: string;
  opportunities: number;
  taken: number;
  missed: number;
  allowed: number;
  example_missed: ProfileExample[];
  example_allowed: ProfileExample[];
}

export interface StructureRow {
  flag: string;
  games: number;
  score_pct: number;
  examples: ProfileExample[];
}

export interface EcoRow {
  eco: string;
  games: number;
  score_pct: number;
  examples: ProfileExample[];
}

export interface Conversion {
  winning_reached: number;
  converted_wins: number;
  losing_reached: number;
  held: number;
}

export interface PlayerProfile {
  player: string;
  games: number;
  score_pct: number;
  eval_coverage_pct: number;
  acpl_opening: PhaseAcpl;
  acpl_middlegame: PhaseAcpl;
  acpl_endgame: PhaseAcpl;
  motifs: MotifRow[];
  structures: StructureRow[];
  eco: EcoRow[];
  conversion: Conversion;
}

export function buildProfile(player: string): Promise<PlayerProfile> {
  return invoke<PlayerProfile>("build_profile", { player });
}

export interface CachedProfileInfo {
  player: string;
  builtAt: string;
}

/** Build + cache the SELF profile so Home's findings panel populates.
 * Call it whenever the self profile is (re)built; `home_summary` only
 * ever reads the cache. */
export function cacheProfile(player: string): Promise<CachedProfileInfo> {
  return invoke<CachedProfileInfo>("cache_profile", { player });
}

/* ---- Phase 5: Repertoire Trainer (Train tab) ---- */

export interface TrainCounts {
  due: number;
  total: number;
}

/** Due/total card counts per color (tab badge + queue header). */
export interface TrainSummary {
  white: TrainCounts;
  black: TrainCounts;
}

/** Next-interval preview per grade in RAW days (the real FSRS scheduler —
 * equal to what grading will set); the UI formats via lib/train.ts. */
export interface GradePreviews {
  again: number;
  hard: number;
  good: number;
  easy: number;
}

export interface DueCard {
  cardId: number;
  repertoireName: string;
  /** Position the expected move is played from (side to move = color). */
  fen: string;
  expectedSan: string;
  expectedUci: string;
  ply: number;
  /** Numbered SAN of the moves leading here (the review prompt). */
  linePrefix: string;
  due: string;
  /** True until the card's first review. */
  isNew: boolean;
  reps: number;
  lapses: number;
  /** Per-grade next intervals for the grade-row buttons. */
  previews: GradePreviews;
}

export interface TrainGraded {
  cardId: number;
  stability: number;
  difficulty: number;
  intervalDays: number;
  due: string;
  reps: number;
  lapses: number;
}

export interface TrainAdded {
  repertoire: string;
  cardsAdded: number;
  cardsExisting: number;
}

export type TrainColor = "white" | "black";
export type TrainGrade = "again" | "hard" | "good" | "easy";

export function trainSummary(): Promise<TrainSummary> {
  return invoke<TrainSummary>("train_summary");
}

export function trainQueue(color: TrainColor, limit?: number): Promise<DueCard[]> {
  return invoke<DueCard[]>("train_queue", { color, limit });
}

export function trainGrade(cardId: number, grade: TrainGrade): Promise<TrainGraded> {
  return invoke<TrainGraded>("train_grade", { cardId, grade });
}

export function trainAddLine(
  color: TrainColor,
  sans: string[],
  startFen?: string,
  name?: string,
): Promise<TrainAdded> {
  return invoke<TrainAdded>("train_add_line", { color, sans, startFen, name });
}

/* ---- Round 2: Home (Direction A) — honest data only ---- */

export interface HomeLastGame {
  id: number;
  white: string;
  black: string;
  ply: number;
  openedAt: string;
}

export interface HomeNewGame {
  id: number;
  white: string;
  black: string;
  result: string;
  source: string;
  /** "personal" | "twic" | "online" | "other". */
  sourceKind: string;
  importedAt: string;
}

export interface HomeFinding {
  label: string;
  value: string;
  evidenceCount: number;
  /** Claim id the Profile screen pre-selects (ViewParams.claim). */
  claimId: string;
}

export interface HomeRunningJobs {
  pending: number;
  running: number;
  done: number;
  failed: number;
  workerActive: boolean;
}

export interface HomeSummary {
  lastGame: HomeLastGame | null;
  /** ≤ 8 rows; the full count rides in newGamesTotal. */
  newGames: HomeNewGame[];
  newGamesTotal: number;
  findingsAvailable: boolean;
  /** ≤ 4, from a CACHED profile only — never built on the fly. */
  findings: HomeFinding[];
  profilePlayer: string | null;
  profileBuiltAt: string | null;
  dueSrs: number;
  /** Always null: the tactics queue is endless — there is no honest "due
   * today" count. Never fake a numeral for it. */
  dueTactics: number | null;
  runningJobs: HomeRunningJobs;
}

export function homeSummary(): Promise<HomeSummary> {
  return invoke<HomeSummary>("home_summary");
}

/** Record the game/ply on the board (feeds Home's Continue card). */
export function touchLastGame(gameId: number, ply: number): Promise<void> {
  return invoke<void>("touch_last_game", { gameId, ply });
}

export interface Commitment {
  /** Free text, e.g. "Club night Thursday". Null = not set. */
  label: string | null;
  opponent: string | null;
}

export function commitmentGet(): Promise<Commitment> {
  return invoke<Commitment>("commitment_get");
}

/** Persist the recurring commitment (meta-backed); null clears a field.
 * Returns the stored state. */
export function commitmentSet(
  label: string | null,
  opponent: string | null,
): Promise<Commitment> {
  return invoke<Commitment>("commitment_set", { label, opponent });
}

export interface PrepEntry {
  opponent: string;
  color: string;
  startedAt: string;
}

export function prepStateGet(): Promise<PrepEntry[]> {
  return invoke<PrepEntry[]>("prep_state_get");
}

/** Replace the stored prep-state list (Home's "no prep started for X"
 * stays truthful — Prep records an entry when step 2 is entered). */
export function prepStateSet(entries: PrepEntry[]): Promise<void> {
  return invoke<void>("prep_state_set", { entries });
}

/* ---- Round 2: database-wide batch operations ---- */

export type BatchKind = "annotate" | "fresh-analysis";

export interface BatchEstimate {
  /** Games the batch would still cover (already queued/done are skipped). */
  games: number;
  perGameMs: number;
  totalEstimateMs: number;
  /** Honesty string: "measured: …" or "assumed: …" — SHOW it verbatim. */
  estimateBasis: string;
}

/** Estimate a batch (no engine, no writes). */
export function batchEstimate(kind: BatchKind): Promise<BatchEstimate> {
  return invoke<BatchEstimate>("batch_estimate", { kind });
}

export interface BatchStarted {
  gamesEnqueued: number;
  jobsEnqueued: number;
  pending: number;
  running: number;
  done: number;
}

/** Enqueue the batch (idempotent); nothing runs until run_jobs. */
export function batchStart(kind: BatchKind): Promise<BatchStarted> {
  return invoke<BatchStarted>("batch_start", { kind });
}

/** Ask the worker to stop between jobs; pending jobs remain (resumable).
 * Returns false when nothing was running. */
export function batchPause(): Promise<boolean> {
  return invoke<boolean>("batch_pause");
}

/* ---- Cosmetics (verdict 4) ---- */

export function setWindowTitle(title: string): Promise<void> {
  return invoke<void>("set_window_title", { title });
}
