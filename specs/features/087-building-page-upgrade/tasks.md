# Tasks — 087 building-page upgrade

Gated by fmt/clippy/`cargo test --workspace`/P11. Branch `feature/087-building-page-upgrade`.

- [x] **T1 — Extract `build_row`** (+ `building_effect`/`field_effect`) as standalone fns; village handler uses them.
- [x] **T2 — `_upgrade.html`** shared panel partial (working `…/build` form, effect/cost/gate/countdown).
- [x] **T3 — Generic `detail.html`** + `building_detail`/`field_detail` handlers + `building_blurb`; `_icons.html`
  symbol sheet shared with the village plan; routes `/building/{kind}` + `/field/{slot}`.
- [x] **T4 — Functional pages** (smithy/academy/troops/rally/market): handlers pass `upgrade`; templates swap the
  "Raise" card for `{% include "_upgrade.html" %}`.
- [x] **T5 — Village plan → links**; remove the inspector + its JS.
- [x] **T6 — `build_submit`** returns to the validated `back` leaf (else the target's page); `is_safe_leaf` guard.
- [x] **T7 — Tests**: migrate the 4 inspector-era tests to the detail pages; add the build-from-page + back-safety test.
- [ ] **T8 — Gate + reviewer + PR.**
