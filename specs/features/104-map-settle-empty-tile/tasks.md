# Tasks — 104 map settle empty tile

Branch `feature/104-map-settle-empty-tile`.

- [x] **T1**: `map_cells` — an empty `Valley` tile gets the Rally Point `href` + `settle=true` + a "free
  valley" label; `MapCellView.settle` (Serialize).
- [x] **T2**: map.html — emit `data-settle`; the inspector's send button reads "Send settlers →" when settle,
  else "Send troops →" (server grid + JS render + select()).
- [x] **T3 — Verify**: live — empty valley → "Send settlers →" (rally href); village/oasis → "Send troops →".
  `/map/tiles` test asserts valley `settle=true`+href and village `settle=false`.
- [ ] **T4 — Reviewer + PR.**
