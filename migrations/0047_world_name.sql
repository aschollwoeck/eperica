-- Per-world display name (056): a human label for a world, set by the admin on creation and shown in the
-- lobby/nav/admin. The URL still routes by the world's UUID; this is display-only, so no uniqueness needed.
-- Existing worlds default to '' and the oldest (home) world is backfilled a friendly name.
ALTER TABLE worlds ADD COLUMN name text NOT NULL DEFAULT '';

UPDATE worlds SET name = 'Home World'
WHERE id = (SELECT id FROM worlds ORDER BY created_at, id LIMIT 1) AND name = '';
