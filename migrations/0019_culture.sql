-- Slice 013 (T4): the per-player culture-point accumulator (GDD §11.1). Culture points are pooled
-- across a player's villages (unlike resources) and produced over time by their buildings. Lazy, like
-- resources (002): stored as value + lastUpdated, computed on read; the *rate* is derived live from
-- the villages' Town Hall levels, so the row is re-anchored (settled to `now`) whenever the rate
-- changes — before a Town Hall completes, or when a village is founded/lost — keeping the read exact.

CREATE TABLE player_culture (
    player_id  uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    value      bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now()
);
