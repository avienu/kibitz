-- Sync history (2026-08-02 field report: "is there a log of syncs? I
-- thought I had twic set to pretty automatic — how can I tell that it's
-- been downloading stuff or not?").
--
-- Every sync stored only its LAST outcome, in a meta key it overwrote.
-- A sync that runs on its own and finds nothing new is indistinguishable
-- from one that never ran, which is exactly the doubt above. One row per
-- run, kept.
CREATE TABLE sync_runs (
    id                 INTEGER PRIMARY KEY,
    -- "twic" | "lichess" | "chesscom" | "fics".
    service            TEXT NOT NULL,
    -- "manual" | "auto" — an automatic pass that found nothing is still
    -- evidence the schedule is alive, and must be visible as such.
    trigger            TEXT NOT NULL CHECK (trigger IN ('manual', 'auto')),
    started_at         TEXT NOT NULL,
    finished_at        TEXT,
    games_imported     INTEGER NOT NULL DEFAULT 0,
    duplicates_skipped INTEGER NOT NULL DEFAULT 0,
    games_failed       INTEGER NOT NULL DEFAULT 0,
    -- Human summary of what the run did ("up to date — TWIC 1655 is the
    -- newest published issue"), or NULL when the counts say it all.
    detail             TEXT,
    -- NULL on success. A failed run is history too.
    error              TEXT
);
CREATE INDEX idx_sync_runs_service ON sync_runs (service, id DESC);
