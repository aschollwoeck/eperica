-- Slice 004: per-village unit research (Academy) and unit upgrade levels (Smithy), plus the
-- research/upgrade order queues as due-timestamped rows (P1).

CREATE TABLE village_research (
    village_id    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    unit_id       text NOT NULL,
    researched_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (village_id, unit_id)
);

CREATE TABLE village_unit_levels (
    village_id uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    unit_id    text NOT NULL,
    level      smallint NOT NULL,
    PRIMARY KEY (village_id, unit_id)
);

CREATE TABLE unit_orders (
    id           uuid PRIMARY KEY,
    village_id   uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    kind         text NOT NULL CHECK (kind IN ('research', 'smithy')),
    unit_id      text NOT NULL,
    target_level smallint,
    complete_at  timestamptz NOT NULL,
    status       text NOT NULL DEFAULT 'pending',
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- One active order per queue kind per village, race-proof (P4): one research AND one smithy
-- upgrade may run concurrently, but never two of the same kind.
CREATE UNIQUE INDEX one_active_unit_order_per_kind
    ON unit_orders (village_id, kind) WHERE status = 'pending';

-- Claim order for the due-event processor (P11).
CREATE INDEX unit_orders_due ON unit_orders (status, complete_at, id);
