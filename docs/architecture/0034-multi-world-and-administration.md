# Multi-world & administration — many concurrent worlds, one account, an admin console

**Status:** Accepted (program design) · **Date:** 2026-06-15 · **Slices:** 036–041

## Context
Eperica is a round-based game (GDD §13.3): a world is created, runs through its artifact/Wonder phases,
ends on the win condition, and is archived to make way for the next round. For a **production launch** we
need (a) **many worlds running concurrently** — at different speeds, like Travian's "game worlds" — that a
single account can play **across simultaneously**, and (b) an **Administrator** console to create,
configure, start and archive them. `specs/roles.md` already defines the **Administrator (Operator)** role
with exactly this remit; it has never been implemented (only the **Moderator** role exists today).

The codebase is **single-world at runtime**, and that is the crux:

- `villages` is already world-scoped (`world_id`, `UNIQUE(world_id, x, y)`), and `users` is already a
  **global account** (no `world_id`) — both the right shape.
- **But there is no per-world "player" entity.** `villages.owner_id → users.id` directly, and all
  *player-level* state (culture points, loyalty, beginner-protection, quest progress, ranking, medals,
  **alliance membership**) is keyed on `users.id`, not on `(user, world)`. The domain's `PlayerId` **is**
  the user id.
- The world's **speed, deterministic map, and scheduler are pinned into `AppState` at boot** from
  `WORLD_SPEED`/`WORLD_RADIUS` env. `scheduled_events` has **no `world_id`** — the scheduler is global.

So "one account, many worlds" is fundamentally an **account ↔ player split** plus a **runtime that holds
many worlds at once**. This is a multi-slice program, not a feature; doing it in one slice would defeat
the per-slice reviewer/PR gate (`specs/README.md` §11).

## Decision

### 1. Account ↔ Player split (the keystone)
- **`users`** remains the global identity: credentials, `is_moderator`, **new `is_admin`**. One row per
  human, world-agnostic.
- **New `players` table**: one row per `(user_id, world_id)` — the per-world game profile. Every
  player-level column/table re-keys from `user_id` to `players.id`: `villages.owner_id`, culture, loyalty,
  protection, quests, ranking, medals, alliance membership, sitting. The domain `PlayerId` becomes the
  **player-row id**, not the user id. (`AuthUser`/`RealUser` continue to resolve the human; a new layer
  resolves the human's player in the *selected* world.)
- Backfill: the existing single world's `users` each get one `players` row; all FKs re-point. No
  user-visible change at this step — still one world.

### 2. World context per request
- After login the account **selects a world**; the choice rides in the (encrypted) session, like the sit
  cookie (030). Game handlers resolve the `(user, selected_world)` player and reject if the account has no
  player in that world (until it joins). World context is **server-derived**, never trusted from the client
  body (P4). A "current world" indicator + switcher extend the 035 nav.

### 3. World registry runtime ("hot, don't disturb other worlds")
- Replace the single pinned `AppState.world`/`map`/scheduler with a **registry**:
  `WorldId → WorldRuntime { config, map: Arc<WorldMap>, scheduler }`. Each world keeps its own seed/speed
  and its own deterministic map (still a pure function of seed+radius — no stored tiles, P6).
- `scheduled_events` gains **`world_id`**; each world runs (or is partitioned into) its **own scheduler**
  loop selecting only its due-events. Spinning up world B **inserts a runtime and starts its scheduler**;
  world A's runtime and scheduler are untouched. This is a *registry add*, **not** a mid-flight hot-swap of
  a live world — far more tractable and exactly what "don't disturb other running worlds" requires.
- Archiving a world stops its scheduler and freezes it via the **existing 021 freeze-guard**
  (`won_at`/`action_guard`), reusing that read-only chokepoint rather than inventing a second one.

### 4. Administrator role + console
- **`is_admin`** on `users`, bootstrapped from an **`ADMINS`** env var (mirroring the `MODERATORS`
  bootstrap in `main.rs`), plus in-app promotion. A `require_admin` gate mirrors `require_moderator`
  (`fairplay.rs`). Admin is **additive** to Player/Moderator (roles.md §2).
- `/admin` console (server-authoritative, P4): **worlds** — list/create/configure (speed, radius,
  end-game schedule)/start/archive; **users** — promote/demote moderator & admin, search/inspect/sanction
  accounts (reuse 022 sanctions); **status** — per-world/round status (population, due-event backlog, win
  state).

## Slice decomposition (build order)
Sequenced so low-risk, independently-valuable pieces land first and the heavy refactors are isolated:

| #   | Slice | Risk | Delivers |
|-----|-------|------|----------|
| 036 | **Admin role + dashboard shell** | Low | `is_admin` + `ADMINS` bootstrap, `require_admin`, gated `/admin`: in-app mod/admin promotion + account management, **read-only** world/server status. Operates on the single world; no multi-world yet. Independently useful. |
| 037 | **Account↔player split** | High | The `players` table + migration/backfill; re-key `PlayerId` semantics from user to player. Pure refactor, no user-visible change. The keystone. |
| 038 | **World-scoped event store** | Med | `WorldId` threaded end-to-end; `scheduled_events.world_id`; the event store scoped by world. Foundational; single-world behaviour unchanged. |
| 039 | **World-scoped due processing** | Med | The repo's per-tick due-claims + requeues filter to the repo's world. Behaviour-preserving; the prerequisite for per-world schedulers. |
| 040 | **World registry runtime** | High | Load all worlds at startup; a `WorldRuntime` (map/speed/repo/event-store) per world; a **scheduler per world** concurrently. The web stays on the home world. |
| 041 | **World lifecycle admin** | Med | Create/start/archive worlds live from the dashboard (registry add/remove, no restart, others undisturbed). |
| 042 | **Player multi-world UX** | Med | Post-login world lobby, join-world flow, nav world switcher, and resolving the player per `(user, world)` in the request path. |

036 stands alone (useful even if paused after). 037–040 are the heavy lift that must all land to reach
real multi-world. 041–042 turn it on for admins and players. (038/039 split the original "world context
plumbing"; the request-path player resolution moved to 042 with world selection.)

## Reuse / decisions
- **Reuse the freeze-guard for archival** (021) and **the sanctions/role pattern for admin** (022) rather
  than new mechanisms.
- **Registry-add, not hot-swap.** We never mutate a *live* world's pinned config; we only add/remove whole
  runtimes. This sidesteps the hardest concurrency problem while meeting the stated requirement.
- **Per-world player, global account.** Chosen over a single global player so a round can be archived
  without touching the account, and one human can hold independent progress in many worlds (the GDD model).

## Consequences
- 037 touches a large surface (`PlayerId` is pervasive) but is invisible to users — it is the risk
  concentrate, deliberately isolated from behavior change.
- Every game query becomes world-scoped through the resolved player; cross-world reads (e.g. a global
  account page listing your worlds) go through `users`, not `players`.
- The scheduler moves from one global loop to per-world loops/partitions; the 023 scale work applies
  per world. P5 (stateless app tier, state-per-world in the DB) is preserved — the registry is a cache of
  DB-derived runtimes, rebuilt on boot.
- Until 041, joining/selecting a world is admin-seeded; players gain self-serve world choice last.
