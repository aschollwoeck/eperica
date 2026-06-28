# Tasks — 109 client cost gate

Branch `feature/109-client-cost-gate`.

- [x] **T1**: `BuildRow.cost_gated` (= disabled & not maxed & not busy & unaffordable); `build_row` sets it.
- [x] **T2**: `_upgrade.html` emits `data-cost-*` on the button + `data-cost-note` on the shortfall note when
  cost_gated.
- [x] **T3**: `_ribbon.html` tick re-enables `button[disabled][data-cost-wood]` when the live amounts cover
  the cost (queried each tick, since the ribbon script runs before later DOM parses) + hides the note.
- [x] **T4 — Verify**: live — an unaffordable upgrade button is disabled with data-cost; as wood ticks past
  the cost the button re-enables and the note hides. Test asserts the cost-gated markup.
- [x] **T5**: extend to the Academy (research) + Smithy (unit upgrade): a `cost_gated` row renders a disabled
  cost-bearing button + flagged note instead of a gate span. Verified live (academy button enabled in ~13s as
  resources ticked; note hid). Test `cost_gated_research_and_upgrade_carry_their_cost`.
- [ ] **T6 — Reviewer + PR.**
