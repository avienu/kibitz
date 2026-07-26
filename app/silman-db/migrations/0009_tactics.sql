-- 0009_tactics: Lichess puzzle storage + tactics training (ROADMAP Phase 5).
--
-- Puzzles come from the Lichess puzzle database (CC0-1.0; bundling and
-- redistribution allowed per CLAUDE.md external-data ground rules).
-- Provenance goes through the existing `sources` table, exactly like every
-- other imported dataset.
--
-- Moves convention (Lichess): `moves` is the space-separated UCI line; the
-- FIRST move is the opponent's setup move played from `fen`, and the user
-- solves from the second move onward.

CREATE TABLE puzzles (
    id               INTEGER PRIMARY KEY,
    source_id        INTEGER NOT NULL REFERENCES sources(id),
    lichess_id       TEXT NOT NULL UNIQUE,
    fen              TEXT NOT NULL,
    moves            TEXT NOT NULL,
    rating           INTEGER NOT NULL,
    rating_deviation INTEGER NOT NULL,
    popularity       INTEGER NOT NULL,
    nb_plays         INTEGER NOT NULL DEFAULT 0,
    -- Space-separated Lichess theme tags (e.g. "fork mate mateIn2 short").
    themes           TEXT NOT NULL
);

CREATE INDEX idx_puzzles_rating ON puzzles (rating);

-- Distinct theme tags present in the imported set, maintained at import
-- time so the motif-filter UI never has to scan millions of rows.
CREATE TABLE puzzle_themes (
    theme   TEXT PRIMARY KEY,
    puzzles INTEGER NOT NULL
) WITHOUT ROWID;

-- Woodpecker method: a fixed puzzle set re-solved in timed cycles.
CREATE TABLE woodpecker_sets (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE woodpecker_set_puzzles (
    set_id    INTEGER NOT NULL REFERENCES woodpecker_sets(id),
    puzzle_id INTEGER NOT NULL REFERENCES puzzles(id),
    -- Solve order within the set (0-based).
    position  INTEGER NOT NULL,
    PRIMARY KEY (set_id, position)
);

CREATE TABLE woodpecker_cycles (
    id          INTEGER PRIMARY KEY,
    set_id      INTEGER NOT NULL REFERENCES woodpecker_sets(id),
    cycle_no    INTEGER NOT NULL,
    started_at  TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    UNIQUE (set_id, cycle_no)
);

-- Per-puzzle attempt history. `rating_at_attempt` is the user's tactics
-- rating BEFORE any update this attempt caused; Woodpecker and speed-drill
-- attempts never move the rating (documented in silman_db::tactics).
CREATE TABLE puzzle_attempts (
    id                INTEGER PRIMARY KEY,
    puzzle_id         INTEGER NOT NULL REFERENCES puzzles(id),
    attempted_at      TEXT NOT NULL DEFAULT (datetime('now')),
    solved            INTEGER NOT NULL CHECK (solved IN (0, 1)),
    time_ms           INTEGER NOT NULL,
    rating_at_attempt REAL NOT NULL,
    -- rated | motif | weakness | woodpecker | speed
    mode              TEXT NOT NULL DEFAULT 'rated',
    cycle_id          INTEGER REFERENCES woodpecker_cycles(id)
);

CREATE INDEX idx_puzzle_attempts_puzzle ON puzzle_attempts (puzzle_id);
CREATE INDEX idx_puzzle_attempts_cycle ON puzzle_attempts (cycle_id);

-- The user's tactics rating: a single-row Elo-style ledger. `attempts`
-- counts rating-affecting attempts only (it drives the provisional-K
-- schedule in silman_db::tactics::elo_update).
CREATE TABLE tactics_rating (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    rating     REAL NOT NULL,
    attempts   INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO tactics_rating (id, rating, attempts) VALUES (1, 1500.0, 0);
