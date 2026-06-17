# Feature 058 — public boards accessible to logged-out visitors

**Status:** Verified
**Amends:** 056 (URL-based world selection) — the public-page visitor-access trade-off it flagged.

**Note:** Under 056 the public read pages moved to `/w/{world}/…`, so a logged-out visitor hitting a bare
`/leaderboard` (or the nav's public links) was bounced to `/worlds` — which is login-gated — losing anonymous
access to the boards (the 046 "public read pages"). Restore it: a no-world **public** route resolves to the
**home** world's board (anonymous-viewable), while no-world **game** routes still go to the lobby (games need
login). No domain change (P3).

## Problem

`/leaderboard`, `/wonder` (and their nav links) for a visitor → `/worlds` → `/login`. The boards are public
(no auth needed at `/w/{world}/leaderboard`), but a visitor has no way in without choosing a world, which
requires logging in.

## Goal

- **AC1 — Bare public routes default to the home world.** `GET /leaderboard` and `GET /wonder` (no world) →
  `302 /w/{home}/leaderboard` / `…/wonder` (the home/default world), viewable by an anonymous visitor.
- **AC2 — Bare game routes still go to the lobby.** `GET /village`, `/map` (no world) → `/worlds` (unchanged;
  game pages require login + a joined world).
- **AC3 — Public nav links work for visitors.** The nav's **Leaderboards**/**Wonder** links, when not inside a
  world, point at the bare public route (→ home world), so a logged-out visitor reaches the boards. Inside a
  world they point at `/w/{world}/…` (current world, unchanged). The member **Village** link still falls back
  to `/worlds` when not in a world.

## Design

- `crates/web/src/handlers.rs`: `redirect_to_home_public(state, leaf)`-style handlers
  `redirect_home_leaderboard` / `redirect_home_wonder` → `Redirect::to(&world_path(state.world_id, "/…"))`.
- `crates/web/src/lib.rs`: bare `/leaderboard` → `redirect_home_leaderboard`, `/wonder` →
  `redirect_home_wonder`; `/village`, `/map` keep `redirect_to_lobby`.
- `crates/web/templates/base.html`: the public links use a new `data-wl-public` attribute — the nav JS rewrites
  it to `/w/{world}{leaf}` inside a world, else to the **bare** `{leaf}` (which the server then redirects to
  the home world). `data-wl` (Village) keeps its `/worlds` fallback.

## Out of scope

- Notifications/messages aggregation (059). Choosing which world is the public "default" beyond the home
  world. `/search` (needs a query; not a public nav landing).
