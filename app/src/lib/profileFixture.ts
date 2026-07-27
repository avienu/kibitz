/**
 * Shared PlayerProfile fixture for the Profile/Tactics build-out tests.
 * WeakKing is the loudest motif (missed 6 + allowed 11); the isolated-pawn
 * structure scores 38% — the two findings the lede must name.
 */
import type { PlayerProfile } from "./db";

export const PROFILE_FIXTURE: PlayerProfile = {
  player: "sounix",
  games: 42,
  score_pct: 51.2,
  eval_coverage_pct: 80,
  acpl_opening: { moves: 100, acpl: 24, blunders: 0, mistakes: 3, inaccuracies: 5 },
  acpl_middlegame: { moves: 200, acpl: 61, blunders: 4, mistakes: 9, inaccuracies: 12 },
  acpl_endgame: { moves: 80, acpl: 48, blunders: 2, mistakes: 6, inaccuracies: 4 },
  motifs: [
    {
      kind: "WeakKing",
      opportunities: 9,
      taken: 3,
      missed: 6,
      allowed: 11,
      example_missed: [{ game: 7, ply: 43 }],
      example_allowed: [{ game: 8, ply: 29 }],
    },
    {
      kind: "Undefended",
      opportunities: 5,
      taken: 2,
      missed: 3,
      allowed: 2,
      example_missed: [{ game: 9, ply: 15 }],
      example_allowed: [],
    },
  ],
  structures: [
    { flag: "own-isolated-pawn", games: 22, score_pct: 38, examples: [{ game: 10, ply: 30 }] },
  ],
  eco: [],
  conversion: { winning_reached: 41, converted_wins: 26, losing_reached: 38, held: 8 },
};
