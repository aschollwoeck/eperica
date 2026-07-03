-- 118: AI agent accounts + API keys. `is_ai` marks synthetic accounts bound to automated runners
-- (formalised further in 120); `agent_keys` holds the public key-id half and the SHA-256 hash of
-- the secret — the plaintext is returned exactly once at creation and never persisted.
ALTER TABLE users ADD COLUMN is_ai boolean NOT NULL DEFAULT false;
CREATE TABLE agent_keys (
    id          text PRIMARY KEY,          -- the public key-id half (hex)
    user_id     uuid NOT NULL REFERENCES users(id),
    secret_hash text NOT NULL,             -- sha256(secret), hex
    created_at  timestamptz NOT NULL DEFAULT now(),
    revoked_at  timestamptz
);
CREATE INDEX agent_keys_user ON agent_keys (user_id);
