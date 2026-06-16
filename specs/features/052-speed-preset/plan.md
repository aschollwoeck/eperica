# Plan — 052 admin preset selection + a real `speed` preset

## Tasks (serial, gated per task)

- **T1 — Full preset directories + loader refactor (behaviour-preserving).**
  `git mv` the 17 `WorldRules` balance TOMLs into `specs/balance/presets/classic/` (fairplay stays).
  In `balance.rs`: a `PresetData` struct of `&'static str` per file; `const CLASSIC` via `include_str!` of
  `presets/classic/*`; `preset_data("classic") -> Some(&CLASSIC)`. Split each loader into `parse_*(toml)` +
  a thin no-arg `*_rules()` returning `CLASSIC`'s file. `world_rules.rs` `load_world_rules(preset)` resolves
  `preset_data` and assembles from the `parse_*` cores. Still `KNOWN_PRESETS = ["classic"]`. Full suite green
  (pure relocation/refactor). Touch `db.rs`? No — no migration. (The `include_str!` paths change, forcing a
  rebuild of `balance.rs`.)

- **T2 — The `speed` preset.**
  `specs/balance/presets/speed/` = full copy of the 17 files; edit `lifecycle.toml` (shorter beginner
  protection + inactivity), `units.toml` (faster base unit speeds), `trade.toml` (faster merchant speed). Add
  `const SPEED` + `preset_data("speed")`; `KNOWN_PRESETS = ["classic", "speed"]`. Unit test:
  `load_world_rules("speed")` loads and its `lifecycle.beginner_protection_secs` < `classic`'s.

- **T3 — Admin preset selection (server-authoritative).**
  `admin.rs` `create_world` takes + validates (`known_preset`) + persists `rule_preset`; `world.rs`
  `create_world` writes the column. `CreateWorldForm` gains `preset`; the handler resolves it (default
  `classic`, reject unknown). `admin.html` renders a `<select>` over `KNOWN_PRESETS`. Integration test: admin
  POST with `preset=speed` → the new world's `rule_preset == "speed"` and it is serviceable; an unknown
  preset is rejected.

- **T4 — Slice verification.**
  Full gate (`fmt`/`clippy`/`test --workspace`/P11) + reviewer until APPROVE.

## Gates (every task)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
`cargo test --workspace`; P11 (no per-request balance reload — presets are cached by 050).

## Risks

- **`include_str!` path churn** — all classic paths now point under `presets/classic/`; a missed path is a
  compile error (safe). The re-embed concerns `sqlx::migrate!`, not `include_str!` (recompiles on path change).
- **Accidental classic drift** — the `git mv` must preserve bytes; verify `classic` parity by the unchanged
  full suite + a hash check.
- **Double-counting speed** — the `speed` preset must change *rules*, not re-apply the `GameSpeed` multiplier
  (build/train/research times already scale with it). Limit edits to protection, unit base speed, merchant
  speed.
