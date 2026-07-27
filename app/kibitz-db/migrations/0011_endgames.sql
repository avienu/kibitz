-- 0011_endgames: endgame trainer attempt history + per-drill mastery
-- (ROADMAP Phase 5). NOTE: 0010 is reserved by parallel work; the migration
-- runner applies by per-version bookkeeping, not MAX(version), so the gap
-- is safe.
--
-- Drill definitions live in the bundled curriculum data file
-- (app/kibitz-db/data/endgame_curriculum.json), keyed by stable string ids;
-- the database stores only the user's progress against those ids.

CREATE TABLE endgame_attempts (
    id           INTEGER PRIMARY KEY,
    drill_id     TEXT NOT NULL,
    attempted_at TEXT NOT NULL DEFAULT (datetime('now')),
    solved       INTEGER NOT NULL CHECK (solved IN (0, 1)),
    -- Moves the user played this attempt.
    user_moves   INTEGER NOT NULL,
    time_ms      INTEGER NOT NULL,
    -- Who supplied the opponent's replies: tablebase | heuristic | mixed |
    -- none (the drill ended before the opponent ever moved).
    opponent     TEXT NOT NULL,
    -- How the outcome was policed: tablebase (every user move was probed
    -- for a theoretical-result flip) | terminal (checkmate/stalemate/draw
    -- detection only; documented in kibitz_db::endgame).
    verification TEXT NOT NULL
);

CREATE INDEX idx_endgame_attempts_drill ON endgame_attempts (drill_id);

-- One row per drill the user has attempted. `clean_streak` counts
-- consecutive solved attempts; reaching kibitz_db::endgame::MASTERY_STREAK
-- stamps `mastered_at` (which then persists — mastery is not revoked).
CREATE TABLE endgame_mastery (
    drill_id     TEXT PRIMARY KEY,
    attempts     INTEGER NOT NULL DEFAULT 0,
    solved       INTEGER NOT NULL DEFAULT 0,
    clean_streak INTEGER NOT NULL DEFAULT 0,
    mastered_at  TEXT
) WITHOUT ROWID;
