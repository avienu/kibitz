-- 0006: structured analysis records with engine provenance, and job-queue
-- extensions (fold-back timestamps, wider purpose set).
--
-- Maintainer verdict (run 4): imported databases carry circa-2011 engine
-- evaluations inside comments. Those become first-class 'legacy-import'
-- rows here — retained forever, never overwritten — while fresh runs
-- stamp the actual engine identity and are preferred for display.

CREATE TABLE analyses (
    id         INTEGER PRIMARY KEY,
    game_id    INTEGER NOT NULL REFERENCES games(id),
    -- Mainline ply the evaluation refers to (position after `ply` plies).
    ply        INTEGER NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('legacy-import','fresh')),
    engine     TEXT NOT NULL,
    depth      INTEGER,
    nodes      INTEGER,
    eval_cp    INTEGER NOT NULL,
    pv         TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_analyses_game ON analyses (game_id, ply);

-- SQLite cannot alter CHECK constraints: rebuild jobs with the wider
-- purpose set and a fold-back timestamp.
CREATE TABLE jobs_new (
    id         INTEGER PRIMARY KEY,
    purpose    TEXT NOT NULL CHECK (purpose IN
                 ('wsui-confirm','user-analysis','batch-annotate',
                  'batch-profile','reanalyze')),
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','running','done','failed')),
    result     TEXT,
    folded_at  TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO jobs_new (id, purpose, payload, status, result, created_at, updated_at)
    SELECT id, purpose, payload, status, result, created_at, updated_at FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_new RENAME TO jobs;
CREATE INDEX idx_jobs_status ON jobs (status);
