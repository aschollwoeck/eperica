-- Slice 008: marketplace trade (due-events, P1). Merchants carry a resource bundle to a target
-- village (the `deliver` leg credits its stores, capped to capacity), then travel home empty
-- (the `return` leg frees the merchants). Reuses the 007 movement-engine pattern over resources.

CREATE TABLE trade_movements (
    id             uuid PRIMARY KEY,
    owner_id       uuid NOT NULL REFERENCES users(id),
    kind           text NOT NULL CHECK (kind IN ('deliver', 'return')),
    home_village   uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,  -- sender; merchants belong here
    target_village uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,  -- credited on deliver
    origin_x       integer NOT NULL,
    origin_y       integer NOT NULL,
    dest_x         integer NOT NULL,
    dest_y         integer NOT NULL,
    wood           bigint NOT NULL DEFAULT 0,   -- carried bundle (all 0 on a return leg)
    clay           bigint NOT NULL DEFAULT 0,
    iron           bigint NOT NULL DEFAULT 0,
    crop           bigint NOT NULL DEFAULT 0,
    merchants      integer NOT NULL CHECK (merchants > 0),
    depart_at      timestamptz NOT NULL,
    arrive_at      timestamptz NOT NULL,
    status         text NOT NULL DEFAULT 'in_transit',
    created_at     timestamptz NOT NULL DEFAULT now()
);

-- Claim order for the due-event processor (P11).
CREATE INDEX trade_movements_due ON trade_movements (status, arrive_at, id);
-- The committed-merchant sum for a sender's free-merchant count (P1 compute-on-read).
CREATE INDEX trade_movements_home ON trade_movements (home_village, status);
