# Feature 037 — Account ↔ Player split — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Additive only. Introduce the `players` entity and a resolution layer; **do not** change any gameplay
query or FK target. The single-world invariant `players.id = users.id` makes this behaviour-preserving, so
the existing suite is the regression oracle (it must pass unchanged). No pure-domain rule (identity + I/O).

## Layers

- **Persistence (migration 0043).**
  - `CREATE TABLE players (id uuid PK, user_id uuid REFERENCES users(id) ON DELETE CASCADE, world_id uuid
    REFERENCES worlds(id), tribe text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, world_id))` + index on `(world_id)`.
  - Backfill: `INSERT INTO players (id, user_id, world_id, tribe) SELECT u.id, u.id, w.id, u.tribe FROM
    users u CROSS JOIN (SELECT id FROM worlds LIMIT 1) w` — one player per user in the single world,
    `id = user_id`. Idempotent via `ON CONFLICT (user_id, world_id) DO NOTHING`.
- **Ports / views.** `AccountRepository::player_in_world(user, world) -> Option<PlayerId>` and
  `worlds_of_user(user) -> Vec<PlayerWorld>` (default empty so non-account fakes are untouched). New
  `PlayerWorld { player: PlayerId, world: WorldId, tribe: Tribe }`.
- **Infra.** Implement the two reads on `PgAccountRepository`. Extend `create_account` to insert the player
  row in the same transaction (`id = user_id`, `self.world_id`, the user's tribe).

## Key decisions

- **Reuse the user UUID for the backfill + new registrations** (single-world era), so the ~30 owner/player
  columns need no re-pointing and behaviour is unchanged. Fresh player ids (and the FK re-point onto
  `players`) arrive with world selection (038), when a user can hold more than one player.
- **Keep `users.tribe`** (still read by the existing code); `players.tribe` is backfilled but not yet read.
- **No `scheduled_events.world_id` here** — that is 038's world-scoping step.

## Risk

- The migration must produce exactly one player per user and preserve `owner_id` validity. Verified by a DB
  test (counts + id equality) and by the unchanged full suite.
