-- 0003: custom start positions (needed for full .si4 import; SCID study
-- databases contain games that begin from a set-up position).
-- NULL = standard initial position.
ALTER TABLE games ADD COLUMN start_fen TEXT;
