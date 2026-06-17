# Tasks — 057 per-world freeze guard

- [x] **T1** — `action_guard` parses the target world from `/w/{world}/…` (`world_in_path` helper) and checks
  **that** world's `world_ended()` via the registry's per-world repo; account POSTs (no world) skip the freeze
  check. Test `freeze_is_enforced_per_world` (frozen world B rejects, open home accepts); existing
  `frozen_world_rejects_mutations` still passes. Full gate + reviewer.

Gates: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
