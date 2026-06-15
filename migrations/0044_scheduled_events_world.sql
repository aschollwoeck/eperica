-- World-scoped event store (038 — M9 multi-world & administration).
-- Key each scheduled event to its world so a per-world scheduler (039) only claims/requeues its own
-- events. Add nullable, backfill to the single existing world, then enforce NOT NULL + the FK.
ALTER TABLE scheduled_events ADD COLUMN world_id uuid;

-- Single-world backfill: `LIMIT 1` is unambiguous while exactly one world exists (039 introduces more).
UPDATE scheduled_events
SET world_id = (SELECT id FROM worlds LIMIT 1)
WHERE world_id IS NULL;

ALTER TABLE scheduled_events ALTER COLUMN world_id SET NOT NULL;
ALTER TABLE scheduled_events ADD CONSTRAINT scheduled_events_world_fk
    FOREIGN KEY (world_id) REFERENCES worlds(id);

-- The scheduler claims pending events for *its* world, nearest-due first; lead the index with world_id.
DROP INDEX IF EXISTS scheduled_events_due_idx;
CREATE INDEX scheduled_events_due_idx ON scheduled_events (world_id, status, due_at, seq);
