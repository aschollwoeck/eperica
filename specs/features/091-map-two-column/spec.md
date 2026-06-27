# Feature 091 — rework the map page: village-style two-column + tile inspector aside

## Why

The world-map page put the grid full-width with the clicked-tile inspector as a bar **below** it, and the
tiles grew large. Make it match the village layout: the map where the village map sits (left), the
clicked-tile info as a card in the right **aside**, and smaller tiles.

## Acceptance criteria

- **AC1 — Two-column.** The page uses the village's `.vcols` layout: the map grid (+ legend) in the left
  column, a right `.vrail` aside holding the tile inspector.
- **AC2 — Inspector on the right.** Clicking a tile fills the aside card (its coordinate in the header chip,
  the tile label, and the Send-troops / Centre-here actions) — instead of the old bar below the grid.
- **AC3 — Smaller tiles.** Each map tile is smaller (capped ~64px vs the old full-width growth); the viewport
  grows from 9×9 to **13×13** (`MAP_HALF` 4→6) so the smaller tiles fill the map column and more world shows.
- **AC4 — Responsive.** No horizontal overflow; on a phone the column stacks (map, then the inspector) and the
  13 tiles shrink to fit.

## Out of scope

- The tile data/actions themselves (unchanged); scouting/fog rules.
