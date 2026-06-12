-- Slice 012 (T7): animal regrowth. A cleared, unoccupied oasis regrows its wild animals back toward
-- the seeded strength over time (AC9), so an un-held oasis becomes contested again. The schedule
-- lives on the oasis row: `regrow_at` is the next due tick (NULL when occupied or already at full
-- strength). Occupying an oasis clears it; freeing/clearing one sets it.

ALTER TABLE oases ADD COLUMN regrow_at timestamptz NULL;

-- Claim order for the regrow processor: due, unoccupied oases first (P11).
CREATE INDEX oases_regrow_due ON oases (regrow_at)
    WHERE regrow_at IS NOT NULL AND owner_village IS NULL;
