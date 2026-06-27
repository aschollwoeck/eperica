# Feature 096 — "Send merchant" from a map village tile

## Why

The map tile inspector only offered "Send troops" (the Rally Point). For a village tile you should also be
able to ship resources there — a one-click "Send merchant" to the Marketplace pre-filled with the tile.

## Acceptance criteria

- **AC1 — Button on village tiles.** When the selected tile is a village (any owner: yours, an ally's, an
  enemy's), the inspector shows a "Send merchant →" action alongside "Send troops". Non-village tiles (plain
  terrain, oases) do not show it — you can only ship resources to a village.
- **AC2 — Pre-filled Marketplace.** The link opens the acting village's Marketplace with the target tile
  pre-filled (`/village/{acting}/market?x&y`); the send form's x/y inputs default to the tile.
- **AC3 — No-JS + endpoint.** The cell's `market_href` is server-computed (in `map_cells`) and present in both
  the server-rendered grid and the `/map/tiles` JSON, so it works without JS and over the drag-stream.

## Reuse
- Mirrors the Rally Point's existing `?x&y` target pre-fill (`MapQuery` → `target_x/target_y`).

## Out of scope
- Marketplace mechanics/validation (unchanged; server-authoritative as before).
