# Multi-world & administration — many concurrent worlds, one account, an admin console

**Status:** Accepted (program design); **built & Verified** · **Date:** 2026-06-15 (planned), 2026-06-16
(as-built reconciliation) · **Slices:** 036–046

> **As-built note.** The original plan (below) sized the player-facing layer as a single slice ("042 —
> Player multi-world UX"). During implementation it was elaborated into a **five-slice sub-program (042–046)**
> to keep each piece behind the per-slice reviewer/PR gate. The Decision/Context sections record the *design*;
> the **[As-built refinements](#as-built-refinements-042046)** section at the end records what actually
> shipped and the decisions taken on the way. The whole program (036–046) is built and Verified.

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
| 042 | **Player-FK switch-over** | High | Re-point the per-world game FKs `users(id)→players(id)` so a non-account player (the Natar NPC, or a second-world player) can own game state; the NPC gains a single global reuse-UUID player. Behaviour-preserving. |
| 043 | **Request world-context** | Med | A `world` cookie + `GameContext` extractor resolving the selected world's repo/map/speed + the `(user, world)` player; home-world behaviour unchanged. The per-request multi-world seam. |
| 044 | **Game-handler migration** | Med | Move the ~40 game handlers onto `GameContext`; account handlers stay on `AuthUser`/`RealUser`. Handlers operate in the selected world. |
| 045 | **Player multi-world UX** | Med | Post-login world lobby, join-world flow, nav switcher; re-point the per-row `owner/player→user` **name reads** through `players` for second-world players. |
| 046 | **Multi-world ranking & public pages** | Med | World-scope the aggregate boards + `search` and route the public read pages through a player-less `WorldScope` extractor. Boards/stats reflect the selected world. |

036 stands alone (useful even if paused after). 037–040 are the heavy lift that must all land to reach
real multi-world. 041 turns it on for admins; **042–046 turn it on for players** (the original single
"player UX" slice, elaborated into a sub-program — see [As-built refinements](#as-built-refinements-042046)).
(038/039 split the original "world context plumbing"; the request-path player resolution moved to 043.)

## Reuse / decisions
- **Reuse the freeze-guard for archival** (021) and **the sanctions/role pattern for admin** (022) rather
  than new mechanisms.
- **Registry-add, not hot-swap.** We never mutate a *live* world's pinned config; we only add/remove whole
  runtimes. This sidesteps the hardest concurrency problem while meeting the stated requirement.
- **Per-world player, global account.** Chosen over a single global player so a round can be archived
  without touching the account, and one human can hold independent progress in many worlds (the GDD model).
  **Exception (as-built, 042):** the synthetic **Natar NPC** is a *single global player* (one reuse-UUID
  row owning NPC villages in every world, inserted `ON CONFLICT (id) DO NOTHING`) — it has no account and no
  per-world progress to keep separate, so a per-world NPC player would only invite PK collisions across
  worlds' end-games. See [As-built refinements](#as-built-refinements-042046).

## Consequences
- 037 touches a large surface (`PlayerId` is pervasive) but is invisible to users — it is the risk
  concentrate, deliberately isolated from behavior change.
- Every game query becomes world-scoped through the resolved player; cross-world reads (e.g. a global
  account page listing your worlds) go through `users`, not `players`.
- The scheduler moves from one global loop to per-world loops/partitions; the 023 scale work applies
  per world. P5 (stateless app tier, state-per-world in the DB) is preserved — the registry is a cache of
  DB-derived runtimes, rebuilt on boot.
- Until 041, joining/selecting a world is admin-seeded; players gain self-serve world choice last.

## As-built refinements (042–046)

The single planned "player multi-world UX" slice became a five-slice sub-program. The decisions taken
while building it (recorded here so this ADR matches the code; each is detailed in its slice spec):

1. **The FK switch-over is its own slice (042), not part of 037.** 037 added the `players` table and
   backfilled it, but the *game* FKs still pointed at `users(id)` (the keystone refactor was deliberately
   staged). 042 re-points ~18 game columns (`villages.owner_id`, troop/trade movements, battle reports +
   defenders, scout reports, culture, alliances + members + invitations + threads/posts, population
   snapshots, achievements, quests, notifications) to `players(id)`, preserving `ON DELETE`. **Account-level**
   tables (sitting, messaging, fair-play, notification prefs) stay on `users(id)` — the human, not the
   per-world player. The split "which FK is account-level vs game-level" is the load-bearing distinction.

2. **The reuse-UUID invariant.** In the home world `player.id == user.id` (037 backfilled players with the
   user's own UUID). This is what makes 042–046 *behaviour-preserving*: every re-pointed FK, every
   `JOIN players … JOIN users`, and every board collapses to today's values in the home world, so the full
   existing suite is the regression oracle. New (second-world) players get fresh UUIDs (`player.id !=
   user.id`), which is precisely the case the re-pointing exists to handle.

3. **Single global NPC player (exception to "per-world player").** The Natar NPC owns NPC villages in every
   world via **one** `players` row whose id == the NPC user id (`ON CONFLICT (id) DO NOTHING`). A per-world
   NPC player would PK-collide across worlds at end-game (the artifact/Wonder release runs per world). The
   NPC has no account progress to keep separate, so the global-player downside does not apply to it.

4. **Two per-request seams: `GameContext` (043) and `WorldScope` (046).**
   - **`GameContext`** — the *player-bound* seam for game handlers (044): selected world's repo/map/speed +
     the account's player in it. Exposes both `player` (game state) and `account` (the human, for username/
     protection/activity). Login-required; falls back to the home world if the account hasn't joined the
     selected one. The world rides an **encrypted `world` cookie** (like the 030 sit cookie); selection is
     server-authoritative (`POST /world/select` only honours a world the account joined).
   - **`WorldScope`** — the *player-less, login-less* seam for the public read pages (`leaderboard`,
     `wonder`, `search`, stat pages): the selected world's repo only, defaulting to home, never redirecting,
     so anonymous visitors keep public access while a logged-in player sees their selected world.

5. **Read re-pointing: per-row names (045) vs aggregate boards (046).**
   - **Per-row reads (045)** resolve a *single* game player id → username via `JOIN players p ON p.id =
     <game id> JOIN users u ON u.id = p.user_id` (map owners, reinforcements, battle/scout reports, oasis
     occupants, alliance rosters/invitations, forum authors). World-implicit (the rows already belong to a
     world's entities); home-parity by the reuse-UUID invariant; NPC-safe (the NPC has a `players` row).
   - **Aggregate boards (046)** additionally needed *world-scoping*. The key insight: **a game player id is
     world-specific**, so `JOIN players p ON p.id = <game id> AND p.world_id = $world JOIN users` both
     world-scopes (keeps only this world's players) and resolves the name — no `world_id` column on the
     battle tables (no migration), no per-query village join. Boards return/group the **player id** (home
     parity: `player.id == user.id`). One performance caveat surfaced under the 023 scale guard: the
     population board must drive from the pre-aggregated, already-world-scoped population owners and
     **PK-join** `players` (not `FROM players WHERE world_id`, which regressed to ~2.7s over 10k players).
