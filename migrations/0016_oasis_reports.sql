-- Slice 012 (T4): oasis battle reports on the 009 rails. An oasis battle reuses `battle_reports`, but
-- an *unoccupied* oasis (wild animals) has no defender player or village — so the defender columns
-- become nullable and the oasis's tile + a synthetic label stand in for the joined village. An
-- occupied oasis (T6) still records the owner as the defender, fitting the existing columns.

ALTER TABLE battle_reports ALTER COLUMN defender_player DROP NOT NULL;
ALTER TABLE battle_reports ALTER COLUMN defender_village DROP NOT NULL;

-- The village-less defender's tile + display label (NULL for ordinary village battles, which join
-- the defender village for their coordinate/name instead).
ALTER TABLE battle_reports
    ADD COLUMN defender_x integer NULL,
    ADD COLUMN defender_y integer NULL,
    ADD COLUMN defender_label text NULL;

-- Admit the oasis-attack report kind.
ALTER TABLE battle_reports DROP CONSTRAINT battle_reports_kind_check;
ALTER TABLE battle_reports
    ADD CONSTRAINT battle_reports_kind_check
    CHECK (kind IN ('attack', 'raid', 'oasis_attack'));
