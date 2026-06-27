# Feature 092 — size the map so the whole grid fits on screen

## Why

After the 091 rework the 13×13 map could be taller than the viewport, so it scrolled. Cap the tile size to the
height left on screen (below the topbar + command header, above the legend) so the whole grid is visible
without scrolling.

Presentation only — CSS; no template/handler change (P3).

## Acceptance criteria

- **AC1 — Fits the viewport.** Each tile is capped to `clamp(22px, (100svh − chrome)/13, 56px)` so the 13 rows
  fit the available height; the whole map + legend are visible at common laptop heights (e.g. 768px) without
  scrolling, and the tiles scale up on taller screens.
- **AC2 — Panel hugs the grid (desktop).** In the two-column layout the tiles are a fixed capped size and the
  `.mgrid` shrinks to the grid (`width: fit-content`, centred), so it isn't a wide empty box around a small map.
- **AC3 — Mobile unchanged.** On a phone (single column) the grid stays full-width and the 13 tiles flex-shrink
  to fit the column (no horizontal scroll); the fixed-size/hug rules are desktop-only.

## Out of scope

- The command-header height (the chrome offset is a constant); the viewport tile count (091's `MAP_HALF`).
