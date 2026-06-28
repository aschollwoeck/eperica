# Tasks — 106 rally pre-fill mode

Branch `feature/106-rally-prefill-mode`.

- [x] **T1**: `MapQuery` gains `mode`; the rally handler validates it (default raid) → `RallyTemplate.mode`;
  rally.html `selected` is dynamic per option (dropped the hard-coded raid `selected`).
- [x] **T2**: `map_cells` sets `&mode=…` per tile (valley→settle, own village→reinforce, other→raid, own
  oasis→reinforce, wild oasis→attack).
- [x] **T3 — Verify**: live — each map link's href carries the right mode; the Rally Point pre-selects
  raid/attack/reinforce/settle (and raid fallback for bogus). Tests: map hrefs + rally pre-select.
- [ ] **T4 — Reviewer + PR.**
