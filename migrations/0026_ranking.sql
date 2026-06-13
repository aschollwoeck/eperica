-- Slice 016: ranking facts persisted at battle resolution (computed once, summed on read).
--
-- A battle's **attack points** — the valued defender troops the attacker killed (GDD §11.2) —
-- credited to the attacker. Persisted as a fact on the report, like loot. No backfill is intended:
-- points are forward-looking battle facts (pre-016 battles keep 0 and have no battle_defenders rows;
-- their per-group losses were never retained), and points are awarded at battle time by design.
ALTER TABLE battle_reports ADD COLUMN attack_points bigint NOT NULL DEFAULT 0;

-- Per-defending-player contribution + report (016 AC3/AC4): one row per defending player in a
-- battle — the target's garrison owner (is_owner = true) AND each reinforcing player — recording
-- their own forces, losses, the defensive value they contributed (the apportion weight), and their
-- split of the battle's defense points. This makes the defender metric faithful (defense points are
-- shared among all defenders present, GDD §11.2) and lets a reinforcer read their own report (§9.6).
-- `occurred_at` defaults to now() — the same transaction-constant instant as the battle report —
-- so windowed leaderboards filter both consistently without a join.
CREATE TABLE battle_defenders (
    id             uuid PRIMARY KEY,
    battle_id      uuid NOT NULL REFERENCES battle_reports(id) ON DELETE CASCADE,
    player_id      uuid NOT NULL REFERENCES users(id),
    village_id     uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    is_owner       boolean NOT NULL,
    forces         jsonb NOT NULL,
    losses         jsonb NOT NULL,
    defense_value  bigint NOT NULL,
    defense_points bigint NOT NULL,
    occurred_at    timestamptz NOT NULL DEFAULT now()
);

-- Reinforcer inbox + top-defenders SUM: a player's defender rows, newest first.
CREATE INDEX battle_defenders_player ON battle_defenders (player_id, occurred_at DESC);
-- Render all defenders of one battle.
CREATE INDEX battle_defenders_battle ON battle_defenders (battle_id);
