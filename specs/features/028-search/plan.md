# Feature 028 — Search / who-is — Plan

**Spec:** ./spec.md · **Status:** Verified

A small read-only slice: one search box → players (by username prefix), alliances (by name/tag prefix), and
a coordinate jump. Public data only (P4), bounded index-backed reads (P11), a pure coordinate parser (P3).

## Domain (pure, P3) — `crates/domain/src/search.rs`

- `parse_coordinate(&str) -> Option<Coordinate>` — accepts `x|y`, `(x|y)`, `x,y`, `x y` (trimmed; signed
  ints). No I/O. Unit-tested across the accepted forms + rejections.

## Persistence (migration `0039`)

- A functional index for the prefix scan on the largest table:
  `CREATE INDEX users_username_prefix ON users (lower(username) text_pattern_ops)`.
  (Alliances are few; their existing name/tag uniqueness suffices — no new index.)

## Application (ports + use-cases)

- `AccountRepository::search_players(query, limit) -> Vec<PlayerHit>` (`{ id, name }`) — case-insensitive
  **prefix** on `username`, excluding abandoned + NPC, ordered by name, capped. Default no-op.
- `AllianceRepository::search_alliances(query, limit) -> Vec<AllianceHit>` (`{ id, name, tag }`) — prefix on
  name **or** tag, capped. Default no-op.
- `crates/application/src/search.rs`: `search(accounts, alliances, q) -> SearchResults`
  (`{ players, alliances, coordinate }`). Trims `q`; an empty query returns an empty result (the handler
  shows the prompt). `coordinate` via the domain parser. `PLAYER_LIMIT` / `ALLIANCE_LIMIT` constants.

## Web (`crates/web`)

- `GET /search?q=…` — a public results page (no `AuthUser`): players, alliances, and (if parsed) a "go to
  (x|y)" link. Renders the prompt for an empty query and a "no results" state otherwise.
- A small **search form** in `base.html` (GET `/search`), so it's reachable everywhere.
- Pure read (`GET`) ⇒ no action/rate-limit guard needed.

## Reuse / decisions

- **Prefix match, not substring** — index-friendly (`lower(username) LIKE lower($1) || '%'` backed by the
  functional index) and the natural who-is behaviour; bounded by a small `LIMIT` (P11).
- **Public fields only** — the hits carry just id + display name/tag; links go to the existing public 016
  stat pages, so no new private surface (P4).
- **Coordinate parsing in the pure crate** — reproducible + unit-testable, reused by the handler to build
  the map link.

## Risks / testing

- **Domain tests:** `parse_coordinate` accepts each form, rejects junk + out-of-range/garbage.
- **DB tests:** `search_players` prefix-matches, excludes abandoned/NPC, respects the cap;
  `search_alliances` matches name + tag.
- **Application tests:** empty query → empty results; coordinate detected; results assembled.
- **Web tests:** a query finds a player + an alliance with working links; a coordinate query offers the map
  link; an empty query shows the prompt; a no-match query shows "no results"; the page is reachable without
  login (public).
- **Performance (P11):** each kind is a single bounded, index-backed query; the prefix scan uses the
  functional index.
