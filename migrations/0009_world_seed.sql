-- Slice 006: the world map's seed. The whole terrain is a pure function of this seed (P6); no
-- tiles are stored. Pre-006 worlds are backfilled with a deterministic per-world value so their
-- map is stable (AC6); new worlds get their seed at creation.

ALTER TABLE worlds ADD COLUMN seed bigint;
UPDATE worlds SET seed = hashtextextended(id::text, 0) WHERE seed IS NULL;
ALTER TABLE worlds ALTER COLUMN seed SET NOT NULL;
