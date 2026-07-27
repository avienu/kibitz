-- 0004: source kinds (duplicate-resolution priority) and non-destructive
-- duplicate links (decided 2026-07-25, DECISIONS_NEEDED #3):
-- dedupe on move-sequence + normalized players/date, keep the copy from the
-- highest-priority source (personal > twic > online > other), and record
-- the other copy here instead of silently dropping it.

ALTER TABLE sources ADD COLUMN kind TEXT NOT NULL DEFAULT 'other';

CREATE TABLE duplicates (
    kept_game_id INTEGER NOT NULL REFERENCES games(id),
    source_id    INTEGER NOT NULL REFERENCES sources(id),
    white        TEXT,
    black        TEXT,
    event        TEXT,
    site         TEXT,
    round        TEXT,
    date         TEXT,
    white_elo    INTEGER,
    black_elo    INTEGER,
    recorded_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_duplicates_kept ON duplicates (kept_game_id);
