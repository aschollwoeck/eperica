-- Slice 021: Wonder of the World, victory, and round freeze.
--
-- `wonder_release_at`: when Wonder plans + conquerable sites release (after the artifact date). `won_at`
-- + `winner_alliance_id`: the round's result (NULL ⇒ ongoing). Once set, the world is frozen.
ALTER TABLE worlds
    ADD COLUMN wonder_release_at  timestamptz,
    ADD COLUMN won_at            timestamptz,
    ADD COLUMN winner_alliance_id uuid REFERENCES alliances(id);

-- A Natar village flagged as a Wonder-construction site is **conquerable** (unlike artifact vaults) and
-- is where an alliance builds the Wonder.
ALTER TABLE villages ADD COLUMN is_wonder_site boolean NOT NULL DEFAULT false;

-- Capturable Wonder building plans — held in a village, transferred by a winning attack (020 mechanic);
-- holding one gates Wonder construction. SET NULL on village deletion ⇒ the plan drops (unheld).
CREATE TABLE wonder_plans (
    id             text PRIMARY KEY,
    world_id       uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    holder_village uuid REFERENCES villages(id) ON DELETE SET NULL,
    origin_x       integer NOT NULL,
    origin_y       integer NOT NULL,
    released_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX wonder_plans_holder ON wonder_plans (holder_village);
