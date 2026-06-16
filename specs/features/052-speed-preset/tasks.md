# Tasks — 052 admin preset selection + a real `speed` preset

- [x] **T1** — `git mv` 17 balance TOMLs → `presets/classic/`; `PresetData`/`CLASSIC`/`preset_data`;
  `parse_*(toml)` + thin no-arg `*_rules()`; `load_world_rules(preset)` assembles via `preset_data`. Suite
  green (behaviour-preserving). `KNOWN_PRESETS = ["classic"]` still.
- [x] **T2** — `presets/speed/` full copy with edits (lifecycle protection, unit base speed, merchant speed);
  `SPEED` + `preset_data("speed")`; `KNOWN_PRESETS += "speed"`. Unit test: speed loads, protection < classic.
- [x] **T3** — Admin create-world preset `<select>` (server-authoritative, `known_preset`); `create_world`
  threads `rule_preset` through the use case/port/repo and persists it. Integration test: `preset=speed` →
  world `rule_preset=='speed'`; unknown rejected; form lists presets. (Use case gains a justified
  `too_many_arguments` allow; mock tuple aliased to `CreatedWorld`.)
- [ ] **T4** — Full gate + reviewer → APPROVE.

Gates per task: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
