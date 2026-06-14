# Search / who-is

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 028

## Context
Players need a fast way to reach a **player**, an **alliance**, or a **map tile** by name, tag, or
coordinate. A read-only, public lookup over already-public data (names, tags, coordinates) — never private
state (P4).

## Design
- **One query, three kinds.** A single string is matched against player usernames (prefix), alliance names
  and tags (prefix), and parsed as a coordinate. The results page links each hit to its existing public
  destination: `/stats/player/{id}`, `/stats/alliance/{id}`, and `/map?x=&y=`.
- **Prefix match, index-backed.** The SQL is a case-insensitive **anchored** `LIKE` (`lower(col) LIKE
  lower($1) || '%'`) capped by a small `LIMIT` (P11). Migration 0039 adds a functional index
  `users(lower(username) text_pattern_ops)` so the player scan (the largest table) uses the index;
  alliances are few, so their existing name/tag uniqueness suffices. The user's `%`/`_`/`\` are escaped to
  literals (`... ESCAPE '\'`) so a query can't inject wildcard semantics.
- **Public fields only (P4).** `search_players` excludes abandoned + NPC accounts (consistent with the
  leaderboard) and returns just `(id, name)`; `search_alliances` returns `(id, name, tag)`. No troops,
  resources, email, or other private field is read. The result set is computed server-side.
- **Coordinate parsing in the pure crate (P3).** `domain::parse_coordinate` accepts `x|y`, `(x|y)`, `x,y`,
  and `x y`; the handler uses it to build the map link. Reproducible + unit-tested.
- **A public page + a nav box.** `GET /search?q=…` takes no `AuthUser` (Visitors can search, like the
  leaderboard/stat pages it links to). A blank query renders a prompt; a non-empty query with no matches
  renders a clear empty state. A search box in `base.html` makes it reachable everywhere. Being a `GET`, it
  needs no action/rate-limit guard.

## Persistence (migration 0039)
- `CREATE INDEX users_username_prefix ON users (lower(username) text_pattern_ops)` — makes the anchored,
  case-insensitive username prefix scan index-backed.

## Reuse / decisions
- **Prefix, not substring / fuzzy** — index-friendly and the natural who-is behaviour; fuzzy/full-text
  ranking + autocomplete are deferred.
- **Links to existing public pages** — search introduces no new data surface; it is pure navigation over
  the 016 stat pages + the 006 map.
- **Wildcard escaping** — keeps the `LIKE` query literal even though the parameter itself is already
  injection-safe.

## Consequences
- A single search box that routes to players, alliances, and map tiles, over bounded index-backed reads,
  exposing only public identity.
- **Out of scope (deferred):** fuzzy/typo-tolerant + full-text ranking, pagination, autocomplete; searching
  villages (unnamed), messages, forum posts, reports; quadrant/online filters; saved searches.
