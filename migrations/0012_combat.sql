-- Slice 009: combat resolution. Attack/raid movements (on the 007 troop_movements engine) resolve
-- at arrival into casualties + a battle report; the attacker's survivors return via the `return` kind.

-- Widen the movement-kind constraint to admit the two combat kinds.
ALTER TABLE troop_movements DROP CONSTRAINT troop_movements_kind_check;
ALTER TABLE troop_movements
    ADD CONSTRAINT troop_movements_kind_check
    CHECK (kind IN ('reinforce', 'return', 'attack', 'raid'));

-- One battle report, visible to both the attacker and the defender. Forces/losses are unit→count
-- maps (jsonb); the scalars carry the modifiers so the report is fully explainable (GDD §9.6).
CREATE TABLE battle_reports (
    id                uuid PRIMARY KEY,
    occurred_at       timestamptz NOT NULL DEFAULT now(),
    kind              text NOT NULL CHECK (kind IN ('attack', 'raid')),
    attacker_player   uuid NOT NULL REFERENCES users(id),
    attacker_village  uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    defender_player   uuid NOT NULL REFERENCES users(id),
    defender_village  uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    attacker_won      boolean NOT NULL,
    luck              double precision NOT NULL,
    morale            double precision NOT NULL,
    wall_before       integer NOT NULL,
    wall_after        integer NOT NULL,
    attacker_forces   jsonb NOT NULL,
    attacker_losses   jsonb NOT NULL,
    defender_forces   jsonb NOT NULL,
    defender_losses   jsonb NOT NULL
);

-- Inbox queries: a player's reports as attacker or as defender, newest first.
CREATE INDEX battle_reports_attacker ON battle_reports (attacker_player, occurred_at DESC);
CREATE INDEX battle_reports_defender ON battle_reports (defender_player, occurred_at DESC);
