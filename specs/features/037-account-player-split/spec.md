# Feature 037 — Account ↔ Player split

**Status:** Verified
**Depends on:** 036 (admin/M9 program), 001 (users/villages), the world row
**Roadmap:** M9 multi-world & administration, slice 2 of 6 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The **keystone** of the multi-world program. It introduces the per-world *player*
identity so one account can later play across many worlds. **Pure refactor — no user-visible change**, and
the entire existing test suite must still pass unchanged.

## Problem

Today the code treats the **user as the player**: `villages.owner_id → users.id`, and ~30 owner/player
columns across the schema key game state directly on `users.id`. For one account to play many worlds, the
account (global login) must be separated from the **per-world game profile** (villages, culture, alliance,
ranking…). This slice introduces that profile entity without yet switching any query onto it.

## Key idea — reuse the UUID in the single world (zero re-pointing)

In the one existing world there is exactly **one player per user**, so the backfill sets
**`players.id = users.id`**. Every existing `owner_id`/`player_id` value therefore already identifies the
player, so **no data is re-pointed** and behaviour is bit-for-bit unchanged. The divergence
(`player.id ≠ user.id`) only appears when a user joins a *second* world — which cannot happen until world
selection lands (038). So for the single-world era, a player's id equals their user's id, and `PlayerId`
keeps its current numeric values everywhere while gaining its new *meaning* (a player, not a user).

## Goal

- **AC1 — `players` table.** One row per `(user_id, world_id)` — the per-world game profile (`id`,
  `user_id`, `world_id`, `tribe`, `created_at`; `UNIQUE(user_id, world_id)`). The per-world identity all
  later slices resolve to.
- **AC2 — Backfill.** Every existing user gets exactly one player row in the existing world, with
  `id = user_id` and `tribe = users.tribe`. The migration is idempotent.
- **AC3 — Registration creates the player.** `create_account` inserts the player row in the **same
  transaction** as the user + starting village (`id = user_id`, the world, the chosen tribe), so the
  invariant holds for new accounts. `villages.owner_id` is unchanged (`= user_id = player_id`).
- **AC4 — Resolution layer.** New read ports (tested, not yet wired into gameplay): `player_in_world(user,
  world) → Option<PlayerId>` and `worlds_of_user(user) → Vec<PlayerWorld>` (the worlds a user has a player
  in). These are the seam the world-context slice (038) builds on.
- **AC5 — Behaviour preserved.** No gameplay query is re-pointed; the pure `domain` crate is untouched
  (P3); the full existing test suite passes unchanged. `PlayerId` continues to identify the player
  (numerically the user id in the single world).

## Design

- **Persistence (migration 0043).** Create `players`; backfill from `users` (`id = users.id`, the single
  `worlds` row, `users.tribe`). FK targets are **not** re-pointed in this slice (values already match;
  re-pointing the ~30 owner/player columns onto `players(id)` is deferred to 038's switch-over, when reads
  resolve the player).
- **Ports / repo.** `AccountRepository` gains `player_in_world` + `worlds_of_user` (default empty);
  `create_account` inserts the player row alongside the user/village (reusing its existing `self.world_id`
  and `owner = user_id`). New `PlayerWorld` view (`player_id`, `world_id`, `tribe`).
- **No domain change.** `PlayerId` is unchanged structurally; only its documented meaning shifts. No
  scheduler/world-context change (that is 038/039).

## Out of scope (later M9 slices)

- Resolving the player per request from a selected world, and re-pointing reads/FKs onto `players` (038).
- World-scoping `scheduled_events` + the per-world scheduler/registry (038/039).
- Any second world, world selection UI, or new-world player creation with a fresh id (038/040/041).
