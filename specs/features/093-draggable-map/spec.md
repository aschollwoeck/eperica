# Feature 093 — a draggable, dynamically-loaded world map

## Why

The world map rendered a fixed square grid centred on a coordinate, navigated with "↑N ←W E→ ↓S" buttons that
reloaded the page, and didn't use the width of wide screens. Make it **broader on wide screens** (more columns
than rows) and **drag-to-pan** like Google Maps — hold and move the map, with terrain streaming in — replacing
the N/S/E/W buttons. Both need the same capability: fetch tile data on the fly for an arbitrary rectangular
region around any centre.

## Acceptance criteria

- **AC1 — Rectangular, fills the width.** The map fetches a region sized to its container (a fixed tile px), so
  a wide screen shows more columns than rows. New `viewport_coords_rect`/`map_viewport_rect` (the square
  helpers delegate). The page server-renders a wide initial grid (no-JS / shareable `?x&y`).
- **AC2 — JSON tile endpoint.** `GET /w/{world}/map/tiles?cx&cy&hx&hy` returns the cells for that region as
  JSON (Player-only, P4), reusing the extracted `map_cells` builder; `hx/hy` are clamped (≤18/≤14) so one
  request stays bounded (two batched DB queries, P11).
- **AC3 — Drag to pan.** Hold-and-drag the map (pointer events → mouse/touch/pen) pans it instantly via a CSS
  transform; as the pan carries the view a few tiles, a buffered fetch re-centres seamlessly (no flash). A
  drag doesn't fire a tile click; a click still fills the 091 inspector aside.
- **AC4 — N/S/E/W removed.** The move buttons are gone (drag replaces them); the "Go to x|y" jump stays. No
  horizontal overflow; mobile uses touch-drag (`touch-action: none`).

## Reuse
- `map_cells` extracted from the `map` handler's cell loop — shared by the page + the JSON endpoint.
- `axum::Json(serde_json::json!{…})` (as in `me`); `villages_at`/`oasis_owners_at` take any coords slice.

## Out of scope
- Tile data/scouting rules (unchanged); a minimap / zoom levels.
