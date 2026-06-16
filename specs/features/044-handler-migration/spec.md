# Feature 044 — Game-handler migration to `GameContext`

**Status:** Verified
**Depends on:** 043 (`GameContext` extractor + `world` cookie + registry `context_for`).
**Roadmap:** M9 — sub-program slice 3 of 4 (042 FK → 043 context → **044 handler migration** → 045 lobby).
See [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** 043 migrated `/village` as the proof. This slice migrates the **remaining authenticated
game handlers** (economy, construction, military, trade, map, reports, alliance, forum, quests, wonder
build) from the home `AppState` repo/map/speed + `AuthUser` to `GameContext`, so every game action and
view operates in the **selected** world. Behaviour-preserving in the home world — the existing suite is
the regression oracle. No domain change (P3).

## Problem

After 043 only `/village` is world-aware. Every other game handler still reads the home `AppState.accounts`
/`map`/`world.speed` and uses the home `AuthUser` player, so a player with the `world` cookie set to a
second world would still act in their home world for building, training, marching, trading, alliance, etc.

## Goal

- **AC1 — Authenticated game handlers use `GameContext`.** The ~34 handlers that operate on the player's
  own villages / troops / economy / alliance / forum / reports swap `State<AppState>` + `AuthUser` for
  `State<AppState>` (shared rule sets only) + `GameContext`. World-scoped reads/writes go through
  `ctx.accounts`; the map through `ctx.map`; speed/radius through `ctx.speed`/`ctx.radius`; the game
  identity is `ctx.player`. Account-level reads (the logged-in human's username) use `ctx.account`.
- **AC2 — Home-world behaviour preserved.** In the home world `ctx` equals today's home repo/map/speed and
  `ctx.player == ctx.account == user.id`, so every migrated handler behaves exactly as before. The full
  existing web suite passes unchanged (P3, no domain change).
- **AC3 — Multi-world reach.** With the `world` cookie set to a joined second world, the migrated game
  surfaces (the build queue, training, rally point, marketplace, reports list, alliance, forum, quests)
  operate in **that** world. A regression/integration test drives one game action (a build order) in a
  second world and asserts it lands in the second world's village, not the home one.
- **AC4 — Account-level handlers untouched.** Sitting, messaging, notifications, profile, settings,
  fair-play reporting, moderation, and admin handlers keep `AuthUser`/`RealUser`/`MaybeAuthUser` (the
  human). The `world` cookie does not affect them.

## In scope (migrated to `GameContext`)

Economy/construction: `build_submit`, `academy`, `smithy`, `research_submit`, `smithy_upgrade_submit`.
Military: `troops`, `train_submit`, `rally`, `rally_send`, `rally_return`, `oasis_recall`.
Trade: `market`, `market_send`. Map & reports: `map`, `reports`, `scout_report_detail`, `report_detail`.
Wonder action: `wonder_build_submit`. Quests: `quests_page`. Alliance: `alliance`, `alliance_found`,
`alliance_invite`, `alliance_revoke`, `alliance_respond`, `alliance_leave`, `alliance_disband`,
`alliance_expel`, `alliance_transfer`, `alliance_role`, `alliance_diplomacy`. Forum: `forum_page`,
`forum_new`, `forum_thread_page`, `forum_reply`.

## Out of scope

- **The auth-less cross-player read pages** — `leaderboard`, `wonder` (view), `search_page`,
  `player_stats_page`, `alliance_stats_page`. They have no `AuthUser` (public reads) and render **other**
  players' data through `owner_id → users` joins; world-scoping them needs a player-less world seam **and**
  the `owner→user` read re-pointing through `players`. Both ship in **045** together, so the join fix and
  the world-scoping land as one coherent change.
- The lobby / join / switcher UI (045). Until it ships the `world` cookie is set only by `select_world`
  (043) + tests, so non-home worlds stay unreachable for real users.
- Re-pointing the cross-player repo joins (`reinforcements_at/_of`, leaderboards, search, stat names,
  alliance-member names, battle-report names) through `players` for second-world players — **045**.

## Design

Mechanical, per-handler, behaviour-preserving:

- Signature: `AuthUser(player): AuthUser` → `ctx: GameContext`; keep `State<AppState>` for the shared rule
  sets / hashers / hubs / template (all world-agnostic).
- Body: `state.accounts.as_ref()` → `&ctx.accounts`; `state.accounts` → `ctx.accounts`; `state.map` →
  `ctx.map`; `state.world.speed` → `ctx.speed`; `state.world.radius` → `ctx.radius`. The game identity
  `player` is re-bound from `ctx.player`.
- The single account-level read among the in-scope handlers — `map`'s header username
  (`find_user_by_id`) — keys on `ctx.account`.

No routes change. The migration is grouped into commits by domain area; the suite stays green at each step.
