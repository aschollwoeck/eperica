-- Slice 004 (AC13): build-queue lanes for the Roman parallel-queue trait.
-- Non-Roman orders occupy the single 'all' lane (one active order, as in 003); Roman orders occupy
-- 'field' or 'building' so one of each can run concurrently. The lane is computed server-side.

ALTER TABLE build_orders ADD COLUMN lane text NOT NULL DEFAULT 'all'
    CHECK (lane IN ('all', 'field', 'building'));

DROP INDEX one_active_build;
CREATE UNIQUE INDEX one_active_build_per_lane
    ON build_orders (village_id, lane) WHERE status = 'pending';
