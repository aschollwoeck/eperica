-- Slice 030: account sitting. An owner authorises trusted players to operate their account; every mutating
-- action a sitter takes is recorded for the owner to review. Authorisation is checked per request (P4).

CREATE TABLE account_sitters (
    owner_id   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sitter_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    granted_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_id, sitter_id)
);

-- The sitters a given player may operate for (the "accounts you sit for" list).
CREATE INDEX account_sitters_by_sitter ON account_sitters (sitter_id);

CREATE TABLE sitter_actions (
    id         uuid PRIMARY KEY,
    owner_id   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sitter_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- A short description of the action (e.g. "POST /village/build").
    action     text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- The owner's audit log, most-recent first.
CREATE INDEX sitter_actions_owner ON sitter_actions (owner_id, created_at DESC);
