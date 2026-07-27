-- Generated annotations live beside the movetext, not inside it, so the
-- narrator can regenerate them wholesale without ever touching a comment
-- a human wrote. Export merges them in after the mainline move.
CREATE TABLE narrations (
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    ply        INTEGER NOT NULL,
    text       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (game_id, ply)
);
