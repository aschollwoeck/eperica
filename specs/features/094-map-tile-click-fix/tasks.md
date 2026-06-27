# Tasks — 094 map tile-click fix

CSS/JS only (map.html); branch `feature/094-map-tile-click-fix`.

- [x] **T1**: select on pointerup-tap via `elementFromPoint`; remove the per-tile click listeners + the
  capture-phase click suppressor.
- [x] **T2 — Verify**: live — tap selects the tapped tile (inspector coord matches); drag pans without
  selecting; clippy/fmt/map tests green.
- [ ] **T3 — Reviewer + PR.**
