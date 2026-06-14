-- Slice 028: search / who-is. A functional index so the case-insensitive username **prefix** scan the
-- who-is search runs (lower(username) LIKE lower($1) || '%') is index-backed (P11). text_pattern_ops makes
-- the btree usable for the anchored LIKE. Alliances are few, so their name/tag uniqueness suffices.

CREATE INDEX users_username_prefix ON users (lower(username) text_pattern_ops);
