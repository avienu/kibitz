-- 0014: annotate-time suggestion verification (maintainer field report,
-- 2026-07-29: "even though I'm in annotate mode I don't see a recommended
-- move").
--
-- Run 11's safety layer renders suggestion closings only from
-- engine-cleared candidates when a verdict exists for the ply, and drops
-- statically-marked candidates otherwise; wsui-confirm jobs run only
-- where the tactical screen FIRED, so quiet plan plies almost never
-- produced a closing. Annotate (an explicit user engine action, run-9
-- ruling) now also enqueues bounded 'suggest-verify' jobs at
-- closing-eligible quiet plies, so annotated games actually recommend
-- moves — refuted ones still never appear.

-- SQLite cannot alter CHECK constraints: rebuild jobs with the wider
-- purpose set (adds 'suggest-verify'), exactly as 0006 and 0013 did.
CREATE TABLE jobs_new (
    id         INTEGER PRIMARY KEY,
    purpose    TEXT NOT NULL CHECK (purpose IN
                 ('wsui-confirm','user-analysis','batch-annotate',
                  'batch-profile','reanalyze','book-extension',
                  'suggest-verify')),
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','running','done','failed')),
    result     TEXT,
    folded_at  TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO jobs_new (id, purpose, payload, status, result, folded_at, created_at, updated_at)
    SELECT id, purpose, payload, status, result, folded_at, created_at, updated_at FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_new RENAME TO jobs;
CREATE INDEX idx_jobs_status ON jobs (status);
