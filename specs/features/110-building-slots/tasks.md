# Tasks — 110 slot-based village buildings

Branch `feature/110-building-slots`. Serial, test-first for the domain; gates each task
(fmt, clippy -D warnings, test, P11 budget). Acceptance = the eperica-reviewer agent.

- [x] **T1 — Specs.** This spec + tasks; supersede `003`'s "fixed slot per kind" decision; fulfill
  `game-design.md §3.2` (slot model + multiplicity + demolition). (no code)
- [x] **T2 — Domain.** `slot: u8` on `BuildingSlot`; `VILLAGE_BUILDING_SLOTS = 22` + reserved set
  (Main Building 0, Rally Point 1, Wall 11); `building_max_instances(kind)` (Warehouse/Granary/Cranny →
  unbounded, else 1); `can_place(buildings, slot, kind) -> Result<(), PlacementError>` (in range / empty /
  reserved-slot rules / multiplicity / Residence⟷Palace); **stacked effects** — `capacities()` and cranny
  protection **sum** over instances (replace `.max()`/single-level). Domain tests: single instance == today's
  value; two warehouses sum; base-capacity floor; reserved/multiplicity placement accept+reject.
- [x] **T3 — Infra.** Read `village_buildings.slot` into `BuildingSlot.slot`; founding inserts the template's
  explicit slots; apply a **demolish** (target_level 0 → delete the row, free the slot); the build-apply path
  stays keyed by `(village_id, slot)`.
- [x] **T4 — Application.** Slot-centric `order_build` (validate via `can_place`; reserved kinds derive their
  slot; new construction takes the chosen empty slot; upgrades take the slot's current kind); a `demolish`
  use-case gated by Main Building level (balance `demolish_min_main_building`); reuse the 003 queue + due
  processing.
- [x] **T5 — Web build.** Slot-centric build POST (`slot` is the primary field; `kind` only for new
  construction) + server validation; an empty-slot **build menu** listing the buildable kinds (filtered by
  multiplicity, requirements, reserved/Main-Building exclusions) with cost/gate per kind.
- [x] **T6 — Web plan + demolish.** Render the 22 centre slots **by slot** (fixed CSS positions, reserved
  styling, empty "build here +"); multiple Warehouses each in their slot; a **Demolish** action on a built
  slot's page when the Main Building is high enough.
- [x] **T7 — Migration check.** Existing villages render/function unchanged; a single-Warehouse village's
  capacity is unchanged; live-verify a 2nd Warehouse sums. Optional DB CHECK (slot range / reserved kinds).
- [ ] **T8 — Reviewer + PR + merge + restart.**
