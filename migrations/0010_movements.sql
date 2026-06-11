-- Slice 007: troop movements (due-events, P1) and stationed reinforcements. The non-combat
-- movement engine — send/return reinforcements over the 005 garrison and 006 distance.

CREATE TABLE troop_movements (
    id              uuid PRIMARY KEY,
    owner_id        uuid NOT NULL REFERENCES users(id),
    kind            text NOT NULL CHECK (kind IN ('reinforce', 'return')),
    home_village    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,  -- troops belong here
    deliver_village uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,  -- delivered-to on arrival
    origin_x        integer NOT NULL,
    origin_y        integer NOT NULL,
    dest_x          integer NOT NULL,
    dest_y          integer NOT NULL,
    depart_at       timestamptz NOT NULL,
    arrive_at       timestamptz NOT NULL,
    status          text NOT NULL DEFAULT 'in_transit',
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Claim order for the due-event processor (P11).
CREATE INDEX troop_movements_due ON troop_movements (status, arrive_at, id);
CREATE INDEX troop_movements_owner ON troop_movements (owner_id) WHERE status = 'in_transit';

CREATE TABLE movement_troops (
    movement_id uuid NOT NULL REFERENCES troop_movements(id) ON DELETE CASCADE,
    unit_id     text NOT NULL,
    count       integer NOT NULL CHECK (count > 0),
    PRIMARY KEY (movement_id, unit_id)
);

-- Troops one player has stationed in another village to defend it. They belong to home_village's
-- owner; the host village's owner cannot use them. Multiple waves from the same home merge.
CREATE TABLE reinforcements (
    host_village uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    home_village uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    unit_id      text NOT NULL,
    count        integer NOT NULL CHECK (count > 0),
    PRIMARY KEY (host_village, home_village, unit_id)
);

CREATE INDEX reinforcements_home ON reinforcements (home_village);
