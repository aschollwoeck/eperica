# Tasks — 113 level-by-level demolition
- [x] **T1** — application: `order_demolish` targets `level − 1` (was 0); duration = the removed level's
  build time. apply/infra unchanged (upsert for level > 0; delete at 0). Comment updated.
- [x] **T2** — web: the Demolish button reads per-level ("Demolish to level N−1" / "free the slot").
- [x] **T3** — test: `demolish_is_gated_and_frees_a_slot` now demolishes a level-2 building one level
  (→ 1), then again (→ freed).
- [ ] **T4** — reviewer + PR + merge + restart.
