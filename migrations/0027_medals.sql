-- Slice 017: prestige layer — population snapshots, medals, achievement grants.

-- Per-player population at each settled weekly period (017 AC2). One row per (world, player, period);
-- the PK makes the snapshot idempotent. Powers the climber metric + population-over-time (016 deferral).
-- The latest settled period for a world is MAX(period) here (the settlement is state-driven, not a
-- scheduled_events row, to avoid double-claiming with the generic event processor).
CREATE TABLE population_snapshots (
    world_id   uuid NOT NULL REFERENCES worlds(id),
    player_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    period     bigint NOT NULL,
    population bigint NOT NULL,
    taken_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (world_id, player_id, period)
);
CREATE INDEX population_snapshots_period ON population_snapshots (world_id, period);

-- Permanent medal awards (017 AC3/AC5). UNIQUE (period, category, rank) makes the weekly award
-- idempotent (re-running a period awards nothing twice). The subject is a player or an alliance
-- (polymorphic — no FK on subject_id).
CREATE TABLE medals (
    id           uuid PRIMARY KEY,
    period       bigint NOT NULL,
    category     text NOT NULL,
    rank         integer NOT NULL,
    subject_kind text NOT NULL CHECK (subject_kind IN ('player', 'alliance')),
    subject_id   uuid NOT NULL,
    awarded_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (period, category, rank)
);
CREATE INDEX medals_subject ON medals (subject_kind, subject_id);

-- One-time achievement grants (017 AC8). The PK (player, achievement) is the exactly-once guard, so a
-- grant + its reward apply at most once.
CREATE TABLE player_achievements (
    player_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    achievement_id text NOT NULL,
    granted_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (player_id, achievement_id)
);
