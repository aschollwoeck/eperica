-- 125: spectator mode — the omniscient read-only world view.
-- A new account role (`is_spectator`, additive like Moderator/Administrator) lets an admin-granted
-- observer see the full server state without holding a player on any world. Spectator keys (`spk_`)
-- mirror the agent key design (118): id + SHA-256(secret) at rest, plaintext shown once, revocable.
-- The separate table means an agent key can never authenticate on the spectator surface even by bug.
ALTER TABLE users ADD COLUMN is_spectator boolean NOT NULL DEFAULT false;

CREATE TABLE spectator_keys (
    id          text PRIMARY KEY,          -- the public key-id half (hex)
    user_id     uuid NOT NULL REFERENCES users(id),
    secret_hash text NOT NULL,             -- sha256(secret), hex
    created_at  timestamptz NOT NULL DEFAULT now(),
    revoked_at  timestamptz
);
CREATE INDEX spectator_keys_user ON spectator_keys (user_id);
