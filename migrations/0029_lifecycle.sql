-- Slice 019: account lifecycle — beginner's protection + inactivity/abandonment.
--
-- `protected_until`: the instant beginner's protection ends (NULL once never-granted/ended). A player is
-- protected while now < protected_until; an attack on their village is rejected (019 AC2).
-- `last_activity`: the single activity signal driving the inactivity lifecycle (seeded at spawn,
-- refreshed throttled on authenticated activity). `abandoned_at`: set by the sweep — the account is
-- retired (cannot log in, hidden from rankings) but the row is kept so historical reports stay valid.
ALTER TABLE users
    ADD COLUMN protected_until timestamptz,
    ADD COLUMN last_activity   timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN abandoned_at    timestamptz;

-- The abandonment sweep scans live accounts by idle time.
CREATE INDEX users_last_activity ON users (last_activity) WHERE abandoned_at IS NULL;

-- Sweep watermark (mirrors population_snapshots' role for the 017 settlement): the latest swept period
-- is MAX(period), so the scheduler settles any complete-but-unswept period exactly once.
CREATE TABLE inactivity_sweeps (
    world_id        uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    period          bigint NOT NULL,
    swept_at        timestamptz NOT NULL DEFAULT now(),
    abandoned_count int NOT NULL,
    PRIMARY KEY (world_id, period)
);
