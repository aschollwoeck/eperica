-- Slice 005: the garrison, training batches as due-events (P1), and per-village starvation checks.

CREATE TABLE village_units (
    village_id uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    unit_id    text NOT NULL,
    count      integer NOT NULL CHECK (count > 0),
    PRIMARY KEY (village_id, unit_id)
);

CREATE TABLE training_orders (
    id               uuid PRIMARY KEY,
    village_id       uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    building         text NOT NULL CHECK (building IN ('barracks', 'stable', 'workshop')),
    unit_id          text NOT NULL,
    count_total      integer NOT NULL CHECK (count_total > 0),
    count_done       integer NOT NULL DEFAULT 0,
    per_unit_secs    bigint NOT NULL CHECK (per_unit_secs > 0),
    started_at       timestamptz NOT NULL,
    next_complete_at timestamptz NOT NULL,
    status           text NOT NULL DEFAULT 'active',
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- One running batch per troop building per village, race-proof (P4).
CREATE UNIQUE INDEX one_active_training_per_building
    ON training_orders (village_id, building) WHERE status IN ('active', 'processing');

-- Claim order for the due-event processor (P11).
CREATE INDEX training_orders_due ON training_orders (status, next_complete_at, id);

-- At most one pending depletion check per village (AC7); re-validated at fire time.
CREATE TABLE starvation_checks (
    village_id uuid PRIMARY KEY REFERENCES villages(id) ON DELETE CASCADE,
    due_at     timestamptz NOT NULL,
    status     text NOT NULL DEFAULT 'pending'
);

CREATE INDEX starvation_checks_due ON starvation_checks (status, due_at);
