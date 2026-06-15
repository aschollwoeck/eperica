-- Account ↔ Player split (037 — M9 multi-world & administration).
-- The per-world game profile: one row per (user, world). Today there is a single world, so the backfill
-- sets players.id = users.id — every existing owner_id/player_id value already identifies the player, so
-- no game data is re-pointed and behaviour is unchanged. A user's id and their player's id diverge only
-- when they join a *second* world (a later slice); until then a player's id equals their user's id.
CREATE TABLE players (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    world_id   uuid NOT NULL REFERENCES worlds(id),
    tribe      text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- One game profile per account per world.
    UNIQUE (user_id, world_id)
);

CREATE INDEX players_world_idx ON players (world_id);

-- Backfill: one player per existing user in the single world, reusing the user's UUID (id = user_id).
-- Idempotent — re-running adds nothing.
INSERT INTO players (id, user_id, world_id, tribe)
SELECT u.id, u.id, w.id, u.tribe
FROM users u
CROSS JOIN (SELECT id FROM worlds LIMIT 1) w
ON CONFLICT (user_id, world_id) DO NOTHING;
