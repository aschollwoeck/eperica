# Tasks — 052 admin preset selection + a real `speed` preset

- [ ] **T1** — `git mv` 17 balance TOMLs → `presets/classic/`; `PresetData`/`CLASSIC`/`preset_data`;
  `parse_*(toml)` + thin no-arg `*_rules()`; `load_world_rules(preset)` assembles via `preset_data`. Suite
  green (behaviour-preserving). `KNOWN_PRESETS = ["classic"]` still.
- [ ] **T2** — `presets/speed/` full copy with edits (lifecycle protection, unit base speed, merchant speed);
  `SPEED` + `preset_data("speed")`; `KNOWN_PRESETS += "speed"`. Unit test: speed loads, protection < classic.
- [ ] **T3** — Admin create-world preset `<select>` (server-authoritative, `known_preset`); `create_world`
  persists `rule_preset`. Integration test: `preset=speed` → world `rule_preset=='speed'`; unknown rejected.
- [ ] **T4** — Full gate + reviewer → APPROVE.

Gates per task: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
