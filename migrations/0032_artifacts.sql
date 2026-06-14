-- Slice 020: Natar NPC villages + released artifacts.
--
-- Natar villages reuse the combat engine as ordinary `villages` rows owned by a synthetic NPC user, so
-- attacking them works unchanged; `is_npc`/`is_natar` flag them out of boards/stats/sweep/conquest.
ALTER TABLE users ADD COLUMN is_npc boolean NOT NULL DEFAULT false;
ALTER TABLE villages ADD COLUMN is_natar boolean NOT NULL DEFAULT false;

-- One row per released artifact. `holder_village` is the village currently holding it (a Natar village
-- at release; a player's village once captured). SET NULL on village deletion ⇒ the artifact drops
-- (unheld) rather than vanishing.
CREATE TABLE artifacts (
    id             text PRIMARY KEY,
    world_id       uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    kind           text NOT NULL,
    scope          text NOT NULL,
    magnitude      double precision NOT NULL,
    holder_village uuid REFERENCES villages(id) ON DELETE SET NULL,
    origin_x       integer NOT NULL,
    origin_y       integer NOT NULL,
    released_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX artifacts_holder ON artifacts (holder_village);
