-- Build queue (slice 003): one due-timestamped order per village, applied when due.

CREATE TABLE build_orders (
    id            uuid PRIMARY KEY,
    village_id    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    target_table  text NOT NULL,        -- 'field' | 'building'
    slot          smallint NOT NULL,
    building_type text,                  -- set for building targets (incl. new construction)
    target_level  smallint NOT NULL,
    complete_at   timestamptz NOT NULL,
    status        text NOT NULL DEFAULT 'pending',
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- At most one active (pending) order per village (AC3, race-proof, P4).
CREATE UNIQUE INDEX one_active_build ON build_orders (village_id) WHERE status = 'pending';

-- Claim due orders nearest-first (P11).
CREATE INDEX build_orders_due ON build_orders (status, complete_at, id);
