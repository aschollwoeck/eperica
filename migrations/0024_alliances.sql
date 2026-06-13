-- Slice 015 (T3/T4): alliances, membership, invitations, and diplomacy (GDD §10). An alliance groups
-- players (a per-player grouping, not per-village); each member holds a role + a granular rights
-- bitset. Joining is by invitation. Diplomacy is a pairwise stance between two alliances, stored as a
-- normalised unordered pair so "with itself" and "two stances per pair" are structurally impossible.

CREATE TABLE alliances (
    id         uuid PRIMARY KEY,
    name       text NOT NULL UNIQUE,
    tag        text NOT NULL UNIQUE,
    founder_id uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

-- One row per player ⇒ a player belongs to at most one alliance (the PK enforces it). `role` is one of
-- 'founder' | 'leader' | 'member'; `rights` is the AllianceRight bitset (only meaningful for leaders).
CREATE TABLE alliance_members (
    player_id   uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    alliance_id uuid NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    role        text NOT NULL,
    rights      integer NOT NULL DEFAULT 0,
    joined_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX alliance_members_alliance_idx ON alliance_members (alliance_id);

-- A pending invitation is a row keyed by (alliance, invitee); accept/decline/revoke delete it. Absence
-- = resolved (no status column needed).
CREATE TABLE alliance_invitations (
    alliance_id uuid NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    invitee_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (alliance_id, invitee_id)
);
CREATE INDEX alliance_invitations_invitee_idx ON alliance_invitations (invitee_id);

-- The normalised diplomacy pair: `alliance_lo < alliance_hi` makes self-diplomacy structurally
-- impossible and the composite PK makes two stances for one pair impossible. `stance` is 'war' |
-- 'confederation'; `status` is 'proposed' (a confederation awaiting consent) | 'active'. `proposed_by`
-- records which side offered a confederation (only the *other* side may accept).
CREATE TABLE alliance_diplomacy (
    alliance_lo uuid NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    alliance_hi uuid NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    stance      text NOT NULL,
    status      text NOT NULL,
    proposed_by uuid,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (alliance_lo, alliance_hi),
    CHECK (alliance_lo < alliance_hi)
);
