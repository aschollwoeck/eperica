-- Slice 125 T2: index coverage for the spectator read aggregation (AC5 — every feed query is a fixed
-- set of world-scoped, indexed reads, no whole-world scan per refresh).
--
-- None of these hot tables carry a `world_id` column; the world scope is reached through a per-world
-- FK (troop/trade movements via `owner_id`/`home_village` → `players`/`villages`; build/training
-- orders via `village_id` → `villages`) — the existing partial indexes on those columns already bound
-- the scan to the requesting world's rows (the same pattern the 009/023 due-event claimers use). The
-- indexes below only add the **ordering** column so the capped, soonest-first read is a pure index
-- scan instead of a sort over the world's rows.

-- movements_in_world: soonest-arrival ordering alongside the existing owner_id-only partial index
-- (troop_movements_owner, 007).
CREATE INDEX troop_movements_owner_arrive_idx
    ON troop_movements (owner_id, arrive_at) WHERE status = 'in_transit';

-- shipments_in_world: soonest-arrival ordering alongside the existing (home_village, status) index
-- (trade_movements_home, 008).
CREATE INDEX trade_movements_home_arrive_idx
    ON trade_movements (home_village, status, arrive_at);

-- active_build_orders_in_world: soonest-completing ordering alongside the existing one-active-order
-- partial index (one_active_build, 004).
CREATE INDEX build_orders_village_complete_idx
    ON build_orders (village_id, complete_at) WHERE status = 'pending';

-- active_training_in_world: soonest-next-unit ordering alongside the existing one-active-batch
-- partial index (one_active_training_per_building, 005).
CREATE INDEX training_orders_village_next_idx
    ON training_orders (village_id, next_complete_at) WHERE status IN ('active', 'processing');
