# Tasks — 074 the world-map redesign

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/074-map-redesign`.

- [x] **T1 — Data.** `MapCellView` gains `x`/`y` (the cell's coordinate) for the inspector's center/send links;
  the handler sets them. (AC3)
- [x] **T2 — CSS.** Replace the `<table>` map-cell styling with a tile grid (`.mgrid`/`.mrow`/`.mtile`, the
  `map-grid__cell--*` terrain/village states restyled as tiles), the inspector (`.minspect`), the recenter nav
  (`.map-nav`), and the legend (`.mlegend`). (AC1/AC2/AC3)
- [x] **T3 — Template.** Rewrite `map.html`: the `.vcmd` command header (centre chip + radius + recenter nav +
  Go + ← Village) + the tile grid + the inspector + legend + the click-to-inspect JS. (AC1–AC4)
- [x] **T4 — Tests.** Update `map_view_shows_terrain_and_own_village` for the new structure (`mgrid`/`mtile`/
  `minspect`, the centre chip), keeping the village/self/terrain + recenter + visitor-redirect assertions;
  `map_flags_inactive_villages` still holds. (AC1–AC4)
- [ ] **T5 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright (the tile grid +
  click-to-inspect send/center); `eperica-reviewer` → APPROVE; PR opened.
