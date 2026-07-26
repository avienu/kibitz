-- 0008: Repertoire Trainer (Phase 5 opening SRS).
--
-- A repertoire is per-color; its lines come from games/positions the user
-- marks as repertoire or from an imported PGN study. Every training-color
-- move becomes one card: (position, expected move), keyed by the
-- ep-normalized position hash (src/hash.rs, position_hash_version in meta)
-- stored as the u64 bits cast to INTEGER, same convention as positions.hash.
-- Provenance: each repertoire references a sources row like games do.
-- Scheduling state is FSRS (silman-srs, memory_state_version 1): stability/
-- difficulty are NULL until the first review; every review is logged in
-- repertoire_reviews (the lapse history). Timestamps are UTC
-- "YYYY-MM-DD HH:MM:SS" (SQLite datetime()); a card is due when
-- due <= now, so new cards are due immediately.

CREATE TABLE repertoires (
    id         INTEGER PRIMARY KEY,
    color      TEXT NOT NULL CHECK (color IN ('white', 'black')),
    name       TEXT NOT NULL,
    source_id  INTEGER NOT NULL REFERENCES sources(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (color, name)
);

CREATE TABLE repertoire_cards (
    id            INTEGER PRIMARY KEY,
    repertoire_id INTEGER NOT NULL REFERENCES repertoires(id) ON DELETE CASCADE,
    -- ep-normalized position hash of the position the expected move is
    -- played FROM (u64 bits as INTEGER). One card per position per
    -- repertoire; transpositions collapse onto the same card.
    position_hash INTEGER NOT NULL,
    -- FEN of that position (board display; derivable from the hash only by
    -- replay, so stored denormalized).
    fen           TEXT NOT NULL,
    expected_san  TEXT NOT NULL,
    expected_uci  TEXT NOT NULL,
    -- 0-based ply of the position within the line that created the card.
    ply           INTEGER NOT NULL,
    -- Numbered SAN of the moves leading here (the review prompt).
    line_prefix   TEXT NOT NULL,
    -- FSRS memory state (silman-srs MemoryState); NULL until first review.
    stability     REAL,
    difficulty    REAL,
    due           TEXT NOT NULL,
    reps          INTEGER NOT NULL DEFAULT 0,
    lapses        INTEGER NOT NULL DEFAULT 0,
    last_review   TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (repertoire_id, position_hash)
);
CREATE INDEX idx_repertoire_cards_due ON repertoire_cards (due);

-- Full review log: one row per grading, including lapses (grade 1).
CREATE TABLE repertoire_reviews (
    id            INTEGER PRIMARY KEY,
    card_id       INTEGER NOT NULL REFERENCES repertoire_cards(id) ON DELETE CASCADE,
    reviewed_at   TEXT NOT NULL,
    -- FSRS grade: 1 Again, 2 Hard, 3 Good, 4 Easy.
    grade         INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 4),
    elapsed_days  REAL NOT NULL,
    -- Memory state AFTER this review.
    stability     REAL NOT NULL,
    difficulty    REAL NOT NULL,
    interval_days REAL NOT NULL
);
CREATE INDEX idx_repertoire_reviews_card ON repertoire_reviews (card_id);
