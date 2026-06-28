# Feature 107 — the world map is scoped to the selected village

## Why

The map was world-scoped (`/w/{world}/map`) with no village in the URL, so it defaulted everything to the
**capital**: "Recentre on home" went to the capital, and "Send troops/merchant/settlers" acted from the
capital's Rally Point / Marketplace — never the village the player came from. For a multi-village player this
is wrong: actions should use the **actively selected village**.

## Acceptance criteria

- **AC1 — Village in the URL.** The map is `/w/{world}/village/{village}/map` (+ `/map/tiles`), like every other
  village-scoped page (064). The village page's "Map" link carries its own village.
- **AC2 — Acting + home = the selected village.** The map's "send" shortcuts act from the path village's Rally
  Point / Marketplace; "Recentre on this village" + the distance origin use the path village's coordinate —
  not the capital.
- **AC3 — Bare `/map` still works.** A context-less `/w/{world}/map?x&y` (search / notifications / alliance
  links) renders the same map defaulting to the capital village. Visitors are still redirected to login (P4).

## Out of scope
- An in-map village switcher; the order/target pre-fill (106, unchanged).
