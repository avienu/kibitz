-- 0002: opening tree support, ECO dataset table, TWIC issue tracking.

-- The movetext byte (index into the deterministic legal-move ordering) of
-- the move played FROM this position; NULL at the final position of a game.
-- Together with a ply-0 row per game (added by importers from this version
-- on), this makes the positions index double as the opening-tree source.
-- Databases imported before schema v2 have NULL next_byte everywhere and
-- must be re-imported for opening-tree queries to work.
ALTER TABLE positions ADD COLUMN next_byte INTEGER;

-- Bundled CC0 openings dataset (data/openings/*.tsv), loaded on first open:
-- one row per (opening line, ply) position reached by the line's mainline.
CREATE TABLE openings (
    position_hash INTEGER NOT NULL,
    eco           TEXT NOT NULL,
    name          TEXT NOT NULL,
    ply           INTEGER NOT NULL
);
CREATE INDEX idx_openings_hash ON openings (position_hash);

-- TWIC incremental ingest state: one row per successfully imported issue.
CREATE TABLE twic_issues (
    issue       INTEGER PRIMARY KEY,
    source_id   INTEGER REFERENCES sources(id),
    games       INTEGER NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);
