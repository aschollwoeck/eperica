# Feature 056 — URL-based world selection (replace the hidden `world` cookie)

**Status:** Verified
**Touches:** 043–046 (multi-world request context), 035 (auth-aware nav), 052 (admin create-world pattern).

**Note:** Move the selected world from the encrypted `world` cookie into the **URL path** (`/w/{world}/…`,
where `{world}` is the world's UUID), so world selection is explicit, shareable, and bookmarkable — standard
HTTP, no hidden essential state. Worlds gain a human **display name** (set on creation). The cookie is kept
only as a non-essential "last-visited" UX hint; it never drives game state. No domain change (P3).

## Problem

The selected world is carried in a hidden encrypted `world` cookie, read by the `GameContext`/`WorldScope`
extractors. Essential navigation state is invisible in the URL: you can't bookmark or share "world 2's
leaderboard," and the mechanism is opaque. The user wants standard URL-based selection.

## Goal

- **AC1 — World in the path.** World-coupled routes live under `/w/{world}/…` where `{world}` is the world's
  UUID. Game routes (village/map/alliance/quests/reports/wonder-build) and public read pages
  (leaderboard/search/wonder/stats) all move under the prefix. `GameContext`/`WorldScope` resolve the world
  **from the path**, not the cookie.
- **AC2 — Server-authoritative (P4).** A bad UUID, a world the account hasn't joined, or one the registry
  isn't running → redirect to the lobby `/worlds`. Public pages on an unknown world → `/worlds`. The path
  selects; the server still validates.
- **AC3 — No-world contexts → lobby.** Post-login, the nav's world-links from account pages, and bare world
  routes (`/village`, `/map`, `/leaderboard`, `/wonder`) → `/worlds`.
- **AC4 — World display name.** A `worlds.name` column (set by admin on creation, backfilled for the home
  world) is shown in the lobby/nav/admin. The URL still uses the UUID (no slug uniqueness needed).
- **AC5 — Cookie is non-essential.** The `world` cookie is written as a "last-visited" hint and read **only**
  by `/me` (to point the nav) and the lobby's Resume affordance — never to resolve game state. Clearing it
  must not break any URL.
- **AC6 — Account routes unchanged.** `/messages*`, `/notifications*`, `/profile*`, `/settings*`, `/me`,
  `/sitting*`, `/report`, `/admin*`, `/mod*`, `/logout`, `/`, `/worlds`, `/worlds/join` stay un-prefixed.

## Design

See the approved plan (`docs`/the slice tasks). Key mechanics:
- `Router::nest("/w/{world}", world_router())`; leaf paths unchanged. Existing guard layers stay on the outer
  router (their allow-lists are account-level paths).
- `auth.rs::world_from_path` reads the `{world}` param via `RawPathParams` (arity-agnostic — coexists with a
  handler's own `{id}` Path). The 7 two-capture handlers take a 2-field `Path<ScopedId{world,id}>`.
- `name` column mirrors the 052 `rule_preset` flow (migration `0047_world_name.sql` → World/create_world →
  port → repo → use-case → CreateWorldForm/admin.html → lobby).
- Templates: each world-scoped struct gains `pub world: String` (UUID); links become `/w/{{ world }}/…`.
  base.html nav stays field-free — the 035 `/me` JS gets a `world` field and rewrites world-links.
- Redirects: `redirect_with_village`/`redirect_to_village` + a `world_path(world, rest)` helper take the world;
  account/auth redirects → `/worlds` where no world context exists.
- `POST /world/select` removed (lobby links replace it); `join_world` → `/w/{uuid}/village`.

## Out of scope

- Per-world freeze enforcement in `action_guard` (it checks only the home world today) — flagged for a
  follow-up. World slugs/SEO names beyond the display `name`.
- **Notifications/messages stay home-world-scoped** (a pre-existing limitation of the account-level feed —
  it reads the home world's repo). Notification deep-links therefore target the home world. Per-world
  notifications/messages are a separate follow-up.
- **Public read pages now require a world in the URL.** A logged-out visitor reaches a board via
  `/w/{world}/leaderboard`; a bare `/leaderboard` (no world) → `/worlds` (the lobby, which is login-gated).
  This follows the decision that "public pages move too"; if visitor access to a default world's boards
  without picking one is later wanted, bare public routes could instead redirect to the home world.
