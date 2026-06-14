-- Slice 019: preserve battle history across village deletion (abandonment, 019 AC8 / P6).
--
-- The abandonment sweep deletes an account's villages to free the map. Battle reports are SHARED rows
-- (an attacker and a defender), so cascade-deleting them on village deletion would erase a still-active
-- opponent's report and ranking points. Instead the village references become ON DELETE SET NULL and the
-- report carries its own fallback coordinates (the defender side already had them for oasis targets),
-- so a report — and the opponent's points — survive when one party's village is removed.

-- Attacker fallback coordinates (the defender side already has defender_x/defender_y from 0016).
ALTER TABLE battle_reports
    ADD COLUMN attacker_x integer,
    ADD COLUMN attacker_y integer;

-- Backfill from the still-present villages.
UPDATE battle_reports br SET attacker_x = av.x, attacker_y = av.y
    FROM villages av WHERE av.id = br.attacker_village;
UPDATE battle_reports br SET defender_x = dv.x, defender_y = dv.y
    FROM villages dv WHERE dv.id = br.defender_village AND br.defender_x IS NULL;

-- Village references survive deletion as NULL (the stored coords keep the report readable).
ALTER TABLE battle_reports ALTER COLUMN attacker_village DROP NOT NULL;
ALTER TABLE battle_reports ALTER COLUMN defender_village DROP NOT NULL;
ALTER TABLE battle_reports DROP CONSTRAINT battle_reports_attacker_village_fkey;
ALTER TABLE battle_reports ADD CONSTRAINT battle_reports_attacker_village_fkey
    FOREIGN KEY (attacker_village) REFERENCES villages(id) ON DELETE SET NULL;
ALTER TABLE battle_reports DROP CONSTRAINT battle_reports_defender_village_fkey;
ALTER TABLE battle_reports ADD CONSTRAINT battle_reports_defender_village_fkey
    FOREIGN KEY (defender_village) REFERENCES villages(id) ON DELETE SET NULL;

-- Defender contribution rows (016) likewise survive their village's deletion — preserving the
-- reinforcer's defense points; the row is still tied to its battle report and player.
ALTER TABLE battle_defenders ALTER COLUMN village_id DROP NOT NULL;
ALTER TABLE battle_defenders DROP CONSTRAINT battle_defenders_village_id_fkey;
ALTER TABLE battle_defenders ADD CONSTRAINT battle_defenders_village_id_fkey
    FOREIGN KEY (village_id) REFERENCES villages(id) ON DELETE SET NULL;
