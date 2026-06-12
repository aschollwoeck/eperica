-- Slice 010: scouting. A standalone `scout` movement (scouts only) spies and returns; scouts may also
-- ride an attack/raid (the espionage sub-step runs before the 009 main battle). Espionage is seedless
-- (no luck/morale) — only the attacking scouts can die. Intel is a snapshot at arrival.

-- Admit the standalone scout movement kind.
ALTER TABLE troop_movements DROP CONSTRAINT troop_movements_kind_check;
ALTER TABLE troop_movements
    ADD CONSTRAINT troop_movements_kind_check
    CHECK (kind IN ('reinforce', 'return', 'attack', 'raid', 'scout'));

-- What a scouting movement spies on. NULL for non-scouting movements (reinforce/return, and
-- attack/raid without scouts). Set on a standalone `scout` movement and on an attack/raid carrying
-- scouts (defaults to 'defenses' for combined attacks at send).
ALTER TABLE troop_movements
    ADD COLUMN scout_target text NULL
    CHECK (scout_target IS NULL OR scout_target IN ('resources', 'defenses'));

-- A combined attack's defender battle report flags that scouting also occurred (only when the
-- defender's counter-espionage killed at least one scout — the stealth rule, AC8).
ALTER TABLE battle_reports
    ADD COLUMN scouted boolean NOT NULL DEFAULT false,
    ADD COLUMN scout_target text NULL
        CHECK (scout_target IS NULL OR scout_target IN ('resources', 'defenses'));

-- One intel report. The scouter sees the full row (scouts sent/lost + the revealed intel); the target
-- sees a redacted notification, and only for a *detected standalone* mission (combined attacks notify
-- via the battle report's `scouted` flag instead). `intel` is NULL when no scout survived to return.
CREATE TABLE scout_reports (
    id                uuid PRIMARY KEY,
    occurred_at       timestamptz NOT NULL DEFAULT now(),
    scouter_player    uuid NOT NULL REFERENCES users(id),
    scouter_village   uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    target_player     uuid NOT NULL REFERENCES users(id),
    target_village    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    target_x          integer NOT NULL,
    target_y          integer NOT NULL,
    target_type       text NOT NULL CHECK (target_type IN ('resources', 'defenses')),
    scouts_sent       jsonb NOT NULL,
    scouts_lost       jsonb NOT NULL,
    detected          boolean NOT NULL,
    standalone        boolean NOT NULL,
    intel             jsonb NULL
);

-- Inbox queries: the scouter's own reports, and the target's detected-standalone notifications.
CREATE INDEX scout_reports_scouter ON scout_reports (scouter_player, occurred_at DESC);
CREATE INDEX scout_reports_target ON scout_reports (target_player, occurred_at DESC);
