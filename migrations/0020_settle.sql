-- Slice 013 (T5): settling. Settlers travel to a free valley to found a new village (or bounce home
-- if the tile was taken or the slot lost in flight). A settle movement targets a tile, not a village,
-- so it reuses the nullable deliver_village from 012; the settler group rides movement_troops.

ALTER TABLE troop_movements DROP CONSTRAINT troop_movements_kind_check;
ALTER TABLE troop_movements
    ADD CONSTRAINT troop_movements_kind_check
    CHECK (kind IN ('reinforce', 'return', 'attack', 'raid', 'scout',
                    'oasis_attack', 'oasis_reinforce', 'settle'));
