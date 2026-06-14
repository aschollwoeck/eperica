-- Slice 025: player profiles — an editable public bio on the account. Presence is derived from the
-- existing `last_activity` (019), so no new column is needed for it.
ALTER TABLE users ADD COLUMN bio text NOT NULL DEFAULT '';
