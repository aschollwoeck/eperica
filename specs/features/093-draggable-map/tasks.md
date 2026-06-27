# Tasks — 093 draggable map

Branch `feature/093-draggable-map`. Gated by fmt/clippy/`cargo test --workspace`/P11.

- [x] **T1 — Rect viewport** (`application/src/map.rs`): `viewport_coords_rect`/`map_viewport_rect`; square
  helpers delegate; re-export; unit test (cols≠rows).
- [x] **T2 — Extract `map_cells`** (`handlers.rs`) from the `map` cell loop; the page uses it + a rectangular
  default (`MAP_HALF_X/Y`).
- [x] **T3 — JSON endpoint** `map_tiles` (clamped `hx/hy`) + route; `#[derive(Serialize)]` on `MapCellView`;
  trim `MapTemplate`'s nav fields, add `cols`.
- [x] **T4 — Map page** (`map.html` + `base.css`): `.mviewport`/`.mlayer` draggable layer; pointer-events drag
  + buffered re-centre fetch; rebind click-to-inspect; remove N/S/E/W, keep "Go"; the 091 inspector aside.
- [x] **T5 — Tests**: migrate the map page asserts (`mviewport`/`mlayer`/"drag to explore"); add a `/map/tiles`
  JSON test (dims, cell shape, clamp, auth).
- [ ] **T6 — Gate + reviewer + PR.**
