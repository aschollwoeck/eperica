# Feature 117 — tasks

Gates each task: `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`.

- [x] **T1 — Domain.** Add `BuildRules.crop_field_cost: Vec<ResourceAmounts>` and
  `field_cost(resource, level)` (cropland table for `Crop`, shared `field` table otherwise). Unit test
  `field_cost_differentiates_cropland`. (`crates/domain/src/construction.rs`)
- [x] **T2 — Balance data.** Add `[field.crop_cost]` (7:9:7:2 — L1 70/90/70/20; canonical 1–10, ratio-continued
  11–20) to **both** presets. (`specs/balance/presets/{classic,speed}/construction.toml`)
- [x] **T3 — Load.** Parse `[field.crop_cost]` into `crop_field_cost` (same length as the shared field table).
  Infra test `cropland_has_its_own_cheaper_cost`. (`crates/infrastructure/src/balance.rs`)
- [x] **T4 — Charge (P4).** `order_build` prices a field via `field_cost(field.kind, level)` — cropland charged
  from its own table. (`crates/application/src/build.rs`)
- [x] **T5 — Display.** `build_row` prices a field via `field_cost` (using the field's `res` slug) so the panel
  shows the same cost that will be charged. (`crates/web/src/handlers.rs`)
- [x] **T6 — Verify.** Live: a cropland shows/charges 70/90/70/20, a woodcutter 40/100/50/60. fmt/clippy clean;
  domain/infra/build tests pass.
