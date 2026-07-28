-- 0013: opening-triage book extensions (run 10).
--
-- A book extension is a user-requested deep MultiPV engine analysis of a
-- position where the repertoire ends (a GAP or FRONTIER found by the
-- opening triage): the engine proposes candidate lines the user can adopt
-- into the repertoire as SRS cards. Requests go through the jobs queue
-- (new purpose 'book-extension', explicit user click only — CLAUDE.md #6);
-- completed results are stored durably here so the candidate lines
-- survive restarts.

CREATE TABLE book_extensions (
    id            INTEGER PRIMARY KEY,
    -- ep-normalized position hash of `fen` (src/hash.rs,
    -- position_hash_version in meta), u64 bits stored as INTEGER — the
    -- same convention as positions.position_hash and repertoire_cards.
    position_hash INTEGER NOT NULL,
    -- FEN of the analysed position (the triage GAP/FRONTIER spot).
    fen           TEXT NOT NULL,
    requested_at  TEXT NOT NULL DEFAULT (datetime('now')),
    -- `id name` of the engine that produced the lines.
    engine        TEXT NOT NULL,
    -- Search depth and MultiPV width actually used.
    depth         INTEGER NOT NULL,
    multipv       INTEGER NOT NULL,
    -- JSON array of candidate lines, best first:
    --   [{"sans": ["Nf3", ...], "score_cp": 25, "mate": null}, ...]
    -- score_cp/mate are from the point of view of the side to move in
    -- `fen` (the 'fresh' analyses convention).
    lines         TEXT NOT NULL
);
CREATE INDEX idx_book_extensions_pos ON book_extensions (position_hash);

-- SQLite cannot alter CHECK constraints: rebuild jobs with the wider
-- purpose set (adds 'book-extension'), exactly as 0006 did.
CREATE TABLE jobs_new (
    id         INTEGER PRIMARY KEY,
    purpose    TEXT NOT NULL CHECK (purpose IN
                 ('wsui-confirm','user-analysis','batch-annotate',
                  'batch-profile','reanalyze','book-extension')),
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
