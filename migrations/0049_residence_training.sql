-- 099: settlers/administrators train at the Residence (013) — the application keys their training batch to
-- `residence` (a Palace stands in for a Residence, but the batch is stored under `residence`). The original
-- 0008 CHECK only allowed the troop buildings, so a settler order was rejected by the DB. Widen it.
ALTER TABLE training_orders DROP CONSTRAINT training_orders_building_check;
ALTER TABLE training_orders
    ADD CONSTRAINT training_orders_building_check
    CHECK (building IN ('barracks', 'stable', 'workshop', 'residence'));
