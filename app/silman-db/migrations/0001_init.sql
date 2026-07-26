-- 0001_init: core game database schema (see docs/ARCHITECTURE.md).

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE sources (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    origin      TEXT NOT NULL,
    license     TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE players (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE events (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE sites (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE games (
    id               INTEGER PRIMARY KEY,
    source_id        INTEGER NOT NULL REFERENCES sources(id),
    white_id         INTEGER REFERENCES players(id),
    black_id         INTEGER REFERENCES players(id),
    event_id         INTEGER REFERENCES events(id),
    site_id          INTEGER REFERENCES sites(id),
    round            TEXT,
    date             TEXT,
    -- 0 = *, 1 = 1-0, 2 = 0-1, 3 = 1/2-1/2
    result           INTEGER NOT NULL,
    white_elo        INTEGER,
    black_elo        INTEGER,
    eco              TEXT,
    ply_count        INTEGER NOT NULL,
    -- movetext is 1 byte per ply: index into the deterministic legal-move
    -- ordering defined by silman_db::movebin for encoding_version.
    encoding_version INTEGER NOT NULL,
    movetext         BLOB NOT NULL,
    -- duplicate detection: FNV-1a hashes of normalized headers and of the
    -- move sequence; a game is a duplicate iff both collide.
    header_sig       INTEGER NOT NULL,
    moves_hash       INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_games_dup ON games (moves_hash, header_sig);
CREATE INDEX idx_games_white ON games (white_id);
CREATE INDEX idx_games_black ON games (black_id);

-- Position index: 64-bit Zobrist-style hash of the position after each ply
-- (ply 1..=ply_count; the shared initial position is not indexed).
CREATE TABLE positions (
    position_hash INTEGER NOT NULL,
    game_id       INTEGER NOT NULL REFERENCES games(id),
    ply           INTEGER NOT NULL
);

CREATE INDEX idx_positions_hash ON positions (position_hash);
