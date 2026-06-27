# Tasks — 097 Rally Point form UX

Branch `feature/097-rally-form-ux`.

- [x] **T1**: `RallyUnitRow` gains `is_scout`/`is_catapult`; the rally handler sets them from the unit spec.
- [x] **T2**: rally.html — move the unit table to the top; per-unit "max" button; `data-scout`/`data-catapult`
  on each count input; Order defaults to Raid; `id` on the Spy/Catapult fields.
- [x] **T3**: JS — reveal Spy/Catapult fields from the army (inline `display`, not `[hidden]`, so `.field`'s
  grid doesn't override it); max buttons fill + fire the preview. base.css `.rally-send`.
- [x] **T4 — Verify**: live — raid default, table on top, max fills, fields reveal on scout/catapult/mode;
  rally test asserts the new markup. fmt/clippy/tests green.
- [ ] **T5 — Reviewer + PR.**
