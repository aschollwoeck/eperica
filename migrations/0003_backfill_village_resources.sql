-- Data migration: backfill village_resources for villages created before 0002 added the table.
-- Without this, those villages have no resources row and the /village read path fails.
-- Uses the slice-002 starting amounts; ON CONFLICT leaves already-seeded villages untouched.

INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at)
SELECT id, 750, 750, 750, 750, now()
FROM villages
ON CONFLICT (village_id) DO NOTHING;
