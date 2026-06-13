-- Slice 015 (T8): the alliance incoming-defence view (AC9, `incoming_against`) reads in-transit
-- attack/raid movements by their target village (`deliver_village`). Index that lookup so an alliance
-- page load is a partial-index scan over the in-transit set, not a full table scan (P11) — matching the
-- plan's stated expectation.
CREATE INDEX troop_movements_deliver_in_transit_idx
    ON troop_movements (deliver_village)
    WHERE status = 'in_transit';
