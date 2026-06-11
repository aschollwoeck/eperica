-- Slice 004: tribe choice at registration.
-- Pre-004 accounts (and their villages) are backfilled to Gauls — the recommended beginner tribe,
-- with no 004-relevant trait, so no retroactive advantage (spec AC3).

ALTER TABLE users ADD COLUMN tribe text;
UPDATE users SET tribe = 'gauls' WHERE tribe IS NULL;
ALTER TABLE users ALTER COLUMN tribe SET NOT NULL;
ALTER TABLE users ADD CONSTRAINT users_tribe_check
    CHECK (tribe IN ('romans', 'teutons', 'gauls'));

-- Villages carry their owner's tribe (the column exists since 0001, NULL until now).
UPDATE villages v
SET tribe = u.tribe
FROM users u
WHERE v.owner_id = u.id AND v.tribe IS NULL;
ALTER TABLE villages ADD CONSTRAINT villages_tribe_check
    CHECK (tribe IS NULL OR tribe IN ('romans', 'teutons', 'gauls'));
