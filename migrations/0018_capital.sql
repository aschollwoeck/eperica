-- Slice 013 (T3): the capital. Building a Palace designates a player's capital — the one village that
-- may raise its resource fields past the normal cap (§3.4) and (from 014) cannot be conquered. At most
-- one village per owner is the capital; enforced in the apply (per-owner needs the owner join), not a
-- table constraint.

ALTER TABLE villages ADD COLUMN is_capital boolean NOT NULL DEFAULT false;

-- A player's capital lookup.
CREATE INDEX villages_capital ON villages (owner_id) WHERE is_capital;
