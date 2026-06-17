-- Slice 060: messages aggregated across all the account's worlds. A read watermark must be per-world —
-- reading `global` / `dm:<peer>` / `alliance:<id>` in one world must not clear its unread in another
-- (024 conversation_reads had no world_id, so the watermark bled across worlds once an account plays >1).
--
-- Add world_id, backfill existing rows to the home world (the oldest worlds row — the only world before the
-- multi-world split), then make it part of the primary key so the watermark is per (account, world,
-- conversation).

ALTER TABLE conversation_reads ADD COLUMN world_id uuid REFERENCES worlds(id) ON DELETE CASCADE;

UPDATE conversation_reads
   SET world_id = (SELECT id FROM worlds ORDER BY created_at, id LIMIT 1)
 WHERE world_id IS NULL;

ALTER TABLE conversation_reads ALTER COLUMN world_id SET NOT NULL;

ALTER TABLE conversation_reads DROP CONSTRAINT conversation_reads_pkey;
ALTER TABLE conversation_reads ADD PRIMARY KEY (player_id, world_id, conversation);
