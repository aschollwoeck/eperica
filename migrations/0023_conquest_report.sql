-- Slice 014 (T4): record the loyalty change + the ownership transfer on the battle report (AC10). An
-- attack carrying administrators that wins lowers the target's loyalty (and may conquer the village);
-- the report shows loyalty before -> after and whether the village changed hands. Null for ordinary
-- battles (no administrators / a loss).

ALTER TABLE battle_reports
    ADD COLUMN loyalty_before smallint,
    ADD COLUMN loyalty_after smallint,
    ADD COLUMN conquered boolean NOT NULL DEFAULT false;
