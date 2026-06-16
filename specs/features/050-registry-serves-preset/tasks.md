# Tasks — 050 registry serves each world's rule preset

- [x] **T1** — Registry per-preset bundle cache; `context_for`/`build_and_spawn` use the world's bundle;
  `new(.., boot_preset, boot_rules)`; `main.rs` updated. Unit test for `rules_for` (cache hit + unknown → None).
- [x] **T2** — `GameContext`/`WorldScope` carry `rules: Arc<WorldRules>`, populated from `context_for`.
- [x] **T3** — `handlers.rs` scoped migration: GameContext + `village_view_data` → `ctx.rules`; WorldScope →
  `world.rules`; non-world-scoped handlers keep `AppState.world_rules`. (Also: `join_world` now seeds the new
  village from the **selected** world's `starting_village`, and 26 handlers shed their now-dead `State`.)
- [x] **T4** — Acceptance test `registry_serves_each_worlds_preset_bundle` (per-preset bundle shared across
  same-preset worlds; unknown preset → unserviceable) + full gate + reviewer.

Gates per task: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
