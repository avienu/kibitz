-- Declared name aliases (run 8.5): the same person under lexically
-- unrelated names — OTB "O'Connor, Shawn" vs online handles ("avienu").
-- Groups are user-declared; lexical variants merge automatically in
-- code and need no rows here. Name-based (not player-id) so an alias
-- can be declared before that source is ever imported.
CREATE TABLE alias_groups (
    id    INTEGER PRIMARY KEY,
    label TEXT NOT NULL
);
CREATE TABLE alias_members (
    group_id INTEGER NOT NULL REFERENCES alias_groups(id) ON DELETE CASCADE,
    name     TEXT NOT NULL UNIQUE
);
CREATE INDEX alias_members_group ON alias_members(group_id);
