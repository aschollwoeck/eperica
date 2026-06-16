# Feature 052 — admin preset selection + a real `speed` preset

**Status:** Verified
**Depends on:** 048 (`WorldRules` bundle), 049 (`worlds.rule_preset` + name-aware loader), 050 (registry serves
each world's preset).
**Roadmap:** Per-world configuration program — see [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) §B.
**Program note:** Turn per-world rules **on** for operators. Ship a genuine second preset (`speed`) so the
machinery built in 048–050 is exercised by truly divergent rules, and let an admin pick a world's preset on
creation. Per the ADR-0035 decision (revisited at this slice), balance is authored as a **full preset
directory**: each preset is a complete set of balance files under `specs/balance/presets/<name>/`.

## Problem

After 050 the runtime serves each world's preset bundle, but only `classic` exists, so nothing diverges and
operators can't choose a preset. Two things are missing: (1) a real second preset with its own balance data,
and (2) a server-authoritative way for an admin to assign a preset when creating a world.

## Goal

- **AC1 — Full preset directories.** The shipped balance moves to `specs/balance/presets/classic/` (the 17
  files that feed `WorldRules`); the process-global `fairplay.toml` stays at `specs/balance/` (it is not a
  per-world rule — 048). Each preset is a complete directory; `load_world_rules(preset)` parses the named
  preset's files. Behaviour-preserving for `classic` (same bytes, relocated). The balance loaders are
  refactored to parse from a passed-in TOML string; the existing no-arg `*_rules()` loaders keep returning
  `classic` (their callers — tests, perf — are unchanged).
- **AC2 — A real `speed` preset.** `specs/balance/presets/speed/` ships a complete copy of the 17 files with
  **speed-server** tweaks that are rules, not the raw speed multiplier (which is the separate per-world
  `GameSpeed`): shorter **beginner protection**, faster base **troop movement**, faster **merchant** travel.
  `KNOWN_PRESETS = ["classic", "speed"]`. `load_world_rules("speed")` loads and **differs** from `classic`
  (asserted: shorter protection).
- **AC3 — Admin preset selection (server-authoritative, P4).** The create-world form (036) gains a preset
  dropdown listing `KNOWN_PRESETS`; the handler rejects any value not in the allow-list (`known_preset`) and
  persists the chosen preset to `worlds.rule_preset`. Default = `classic`. A freshly-created `speed` world
  starts (050 registry) under the `speed` bundle.
- **AC4 — No behaviour drift for classic.** The full suite passes; a `classic` world is byte-identical to
  before; only an explicitly-`speed` world diverges.

## Design

- **`specs/balance/presets/classic/`** — `git mv` the 17 `WorldRules` TOMLs here (fairplay stays put).
- **`specs/balance/presets/speed/`** — full copy; edits in `lifecycle.toml` (shorter protection + inactivity),
  `units.toml` (faster unit base speeds), `trade.toml` (faster merchant speed). The rest are identical copies.
- **`balance.rs`** — embed each preset's files into a `PresetData { economy: &str, … }` (`CLASSIC`, `SPEED`)
  via `include_str!`; `preset_data(name) -> Option<&'static PresetData>`. Each loader splits into a
  `parse_*(toml: &str)` core + a thin `pub fn *_rules()` that passes `CLASSIC`'s file (back-compat). `fairplay`
  stays a single global const.
- **`world_rules.rs`** — `load_world_rules(preset)` resolves `preset_data(preset)` (→ `UnknownPreset` if
  absent) and assembles `WorldRules` from the named preset's files via the `parse_*` cores. `KNOWN_PRESETS`
  gains `"speed"`.
- **`admin.rs` / `handlers.rs` / `admin.html`** — `create_world` persists `rule_preset` (validated against
  `known_preset`); `CreateWorldForm` gains `preset: Option<String>` (default `classic`); the form template
  renders a `<select>` of `KNOWN_PRESETS`.

## Out of scope

- End-to-end **acceptance** that two live worlds (classic vs speed) diverge across a played scenario, home
  parity, and flipping the ADR to **Accepted** (053). Per-preset *non-balance* config (speed/radius are
  already per-world via 047/world config). Editing a running world's preset (presets are immutable per world).
