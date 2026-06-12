-- Slice 012: oases. Oases become persisted, contestable entities on the seeded map. An oasis row
-- exists once the tile is fought/occupied (lazy, P1); until then its defenders are the seeded wild
-- animals (re-derivable from the world seed, no stored state). The battle reuses the 009 engine on
-- the 007 movement engine, so the schema additions are: the oasis state + its garrison, oasis
-- movement kinds, and a nullable destination village (an oasis movement targets a tile, not a village).

-- An oasis's persisted state. A row appears the first time the oasis is fought or occupied;
-- `owner_village` NULL ⇒ unoccupied (wild animals defend), set ⇒ occupied (the owner's stationed
-- troops defend). The (x, y) identify the tile within this world.
CREATE TABLE oases (
    world_id     uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    x            integer NOT NULL,
    y            integer NOT NULL,
    owner_village uuid NULL REFERENCES villages(id) ON DELETE SET NULL,
    materialised boolean NOT NULL DEFAULT true,
    PRIMARY KEY (world_id, x, y)
);

-- A holder's occupied oases (for the production bonus + the Outpost capacity check).
CREATE INDEX oases_owner ON oases (owner_village) WHERE owner_village IS NOT NULL;

-- The oasis's current defenders: the materialised (possibly regrown) wild animals while unoccupied,
-- or the owner's stationed reinforcements while occupied. Absent rows ⇒ fall back to the seeded
-- animals (an un-fought oasis has no rows).
CREATE TABLE oasis_garrison (
    world_id uuid NOT NULL,
    x        integer NOT NULL,
    y        integer NOT NULL,
    unit_id  text NOT NULL,
    count    integer NOT NULL CHECK (count > 0),
    PRIMARY KEY (world_id, x, y, unit_id),
    FOREIGN KEY (world_id, x, y) REFERENCES oases (world_id, x, y) ON DELETE CASCADE
);

-- An oasis movement targets a tile, not a village — its destination village is NULL (the dest_x/dest_y
-- already identify the oasis). Existing village movements keep a non-NULL deliver_village.
ALTER TABLE troop_movements ALTER COLUMN deliver_village DROP NOT NULL;

-- Admit the two oasis movement kinds (Return is reused for recalls).
ALTER TABLE troop_movements DROP CONSTRAINT troop_movements_kind_check;
ALTER TABLE troop_movements
    ADD CONSTRAINT troop_movements_kind_check
    CHECK (kind IN ('reinforce', 'return', 'attack', 'raid', 'scout',
                    'oasis_attack', 'oasis_reinforce'));
