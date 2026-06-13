-- Slice 013 (T8, review fix): backfill the per-player culture accumulator (0019) for accounts that
-- registered before it existed (slices 001–012). Without a row, a pre-013 player reads CP anchored at
-- the Unix epoch, which settles `rate × decades` of culture on first read and vaults them past the
-- expansion thresholds (013 AC1/AC4). Seed a zero accumulator anchored at *now* so culture starts
-- accruing from here, like a freshly registered player.

INSERT INTO player_culture (player_id, value, updated_at)
SELECT id, 0, now() FROM users
ON CONFLICT (player_id) DO NOTHING;
