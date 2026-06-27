# Tasks — 099 settler training UI

Branch `feature/099-settler-training-ui`.

- [x] **T1 (domain)**: `training_building_level(trained_in, buildings)` — Palace stands in for Residence for
  the training-speed level; used by `order_train`; unit test.
- [x] **T2 (migration)**: 0049 widens `training_orders_building_check` to allow `residence`.
- [x] **T3 (web)**: `troops_residence` handler + `/residence` route; `troops()` resolves the display building
  (Residence/Palace) while keying the roster/batch to Residence; village-page link; train redirect → /residence.
- [x] **T4 — Verify**: live (page offers + trains a settler; Palace label; redirect); integration test
  (link, settler offered, batch keyed `residence`, redirect, Palace stand-in). fmt/clippy/tests green.
- [ ] **T5 — Reviewer + PR.**
