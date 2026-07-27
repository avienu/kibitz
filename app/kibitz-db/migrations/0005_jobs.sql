-- 0005: persistent analysis job queue (docs/ARCHITECTURE.md, engine
-- manager). Jobs survive restarts; a worker resets stale 'running' rows to
-- 'pending' on startup (resumability).
CREATE TABLE jobs (
    id         INTEGER PRIMARY KEY,
    purpose    TEXT NOT NULL CHECK (purpose IN ('wsui-confirm','user-analysis','batch-annotate')),
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','running','done','failed')),
    result     TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_jobs_status ON jobs (status);
