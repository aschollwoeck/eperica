-- 120: AI player visibility setting per world. `ai_visibility = 'labeled'` surfaces NPC tags on
-- boards/map/stat pages; `'disguised'` renders AI villages byte-identically to human ones (AC3/AC4).
-- Existing worlds default to 'labeled' (AC7 — retroactively explicit rather than implicitly unknown).
ALTER TABLE worlds ADD COLUMN ai_visibility text NOT NULL DEFAULT 'labeled';
