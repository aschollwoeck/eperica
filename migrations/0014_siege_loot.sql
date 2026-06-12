-- Slice 011: siege & loot. Surviving catapults raze a targeted building; surviving attackers loot
-- resources (bounded by carry capacity, minus the defender's Cranny). The loot rides the survivor
-- `return` movement and is credited (capped) at the attacker's village on arrival.

-- The catapult target building chosen at send (a BuildingKind slug); NULL when the attack carries no
-- catapults, or none was chosen (a seeded-random building is hit at resolution).
ALTER TABLE troop_movements ADD COLUMN catapult_target text NULL;

-- Resources a movement carries home as loot — set on the survivor `return` of a raid/attack; 0 for
-- reinforcements and loot-less returns.
ALTER TABLE troop_movements
    ADD COLUMN loot_wood bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_clay bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_iron bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_crop bigint NOT NULL DEFAULT 0;

-- The battle report records what was looted and which building (if any) the catapults razed (GDD §9.5).
ALTER TABLE battle_reports
    ADD COLUMN loot_wood bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_clay bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_iron bigint NOT NULL DEFAULT 0,
    ADD COLUMN loot_crop bigint NOT NULL DEFAULT 0,
    ADD COLUMN razed_building text NULL,
    ADD COLUMN razed_before smallint NULL,
    ADD COLUMN razed_after smallint NULL;
