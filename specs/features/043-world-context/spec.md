# Feature 043 — Request world-context (`GameContext`)

**Status:** Draft
**Depends on:** 040 (registry), 042 (player FKs + `create_player_in_world`)
**Roadmap:** M9 — sub-program slice 2 of 4 (042 FK → **043 context** → 044 handler migration → 045 lobby).
See [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The per-request seam that resolves the **selected world** and the account's player in it.
Behaviour-preserving in the home world (the context equals today's home repo/map/speed/player). The village
page is migrated as the proof; the rest of the game handlers follow in 044.

## Problem

Game handlers use the home `AppState.accounts`/`map`/`world.speed` and the home player. To operate in a
selected world a request needs **that world's** repo/map/speed and the account's **player in that world**.

## Goal

- **AC1 — Registry world-context.** `WorldRegistry` caches each running world's meta (seed/radius/speed) +
  the shared `MapRules`, and exposes `context_for(world_id) -> (PgAccountRepository, Arc<WorldMap>,
  GameSpeed, radius)` built on the spot (the map is generate-on-read, so cheap). `None` for a world the
  registry is not running.
- **AC2 — `GameContext` extractor.** Resolves: the selected world from an encrypted `world` cookie
  (default = home), the effective account (sit-aware), then the account's player via
  `player_in_world(user, world)`. If the account has **no** player in the selected world, it **falls back
  to the home world** (always joined). Yields `{ accounts, map, player, world_id, speed, radius }`.
- **AC3 — Proof migration.** The village page (`/village`) uses `GameContext` instead of the home
  `AppState` fields + `AuthUser`. Home-world behaviour is unchanged (the existing village tests pass).
- **AC4 — Select + multi-world view.** `POST /world/select` sets the `world` cookie to a world the account
  has joined (server-authoritative; an unjoined world is ignored). With a player in a second world and that
  world selected, the village page renders **that world's** village. Account-level behaviour is unaffected
  (the cookie does not change `AuthUser`/`RealUser`). *(The lobby page that lists worlds + posts here is
  045.)*
- **AC5 — Behaviour preserved.** No domain change (P3); the full existing suite passes unchanged.

## Design

- **Registry meta cache.** `start_world` records `WorldMeta { seed, radius, speed }`; the registry holds the
  loaded `MapRules`. `context_for` builds `PgAccountRepository::new(pool, world, seed, radius,
  economy.starting_amounts, beginner_secs, speed)` + `Arc<WorldMap>` from the cache.
- **`world` cookie** (`auth.rs`) — encrypted, like the sit cookie; `world_cookie(id)` / `clear_world_cookie`
  helpers (used by the switcher in 045).
- **`GameContext`** extractor (`auth.rs`): cookie → world (default home); `effective_identity` → account →
  `player_in_world`; home fallback if not joined. The selected world must be one the registry runs
  (`context_for`), else home.
- **Village handler.** Swap `State` + `AuthUser` for `State` (rules/hubs) + `GameContext`; use
  `ctx.accounts`/`ctx.map`/`ctx.speed`/`ctx.radius`/`ctx.player`.

## Out of scope

- Migrating the other ~40 game handlers (044). The lobby / join / switch UI and re-pointing `owner→user`
  reads through `players` for second-world players (045). Until the switcher ships, the `world` cookie is
  set only by tests, so non-home worlds are unreachable by real users.
