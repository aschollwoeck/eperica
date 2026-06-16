# Feature 048 — `WorldRules` bundle (keystone refactor)

**Status:** Draft
**Depends on:** 047 (per-world config program start).
**Roadmap:** Per-world configuration program — see [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) §B.
**Program note:** The keystone for per-world rule presets (mirrors the 037 player split): consolidate the
~19 **sim** rule sets — today loaded individually and shared as separate `Arc`s on `AppState` and the
`WorldRegistry` — into **one** `WorldRules` bundle (`Arc<WorldRules>`). Pure refactor, **no behaviour
change**; the existing suite is the regression oracle. Later slices make the bundle per-world (049–052).

## Problem

The per-world game rules are scattered as ~14 separate `Arc<…>` fields on `AppState` and ~15 params/fields
on `WorldRegistry`. To make rules per-world there must be a single unit that a world can carry; today there
is none. The split also makes the registry constructor a 17-arg function.

## Goal

- **AC1 — One bundle.** A `WorldRules` struct (in `infrastructure`) owns the per-world sim rule sets:
  economy, build, units, combat, culture, loyalty, alliance, ranking, achievements, quests, lifecycle,
  merchant, wonder, oasis, scout, artifacts, medals, map-rules, starting-village. A `load_world_rules()`
  loads them all (= the current/`classic` balance). `fair_play_rules` (process/account-level anti-cheat) and
  the hashers/hubs/proxy/live-`WorldMap` stay **outside** the bundle.
- **AC2 — `AppState` holds the bundle.** `AppState`'s individual rule `Arc`s are replaced by one
  `world_rules: Arc<WorldRules>`; handlers read `state.world_rules.<set>`. `fair_play_rules`, `map`,
  `world`, hashers, hubs, registry stay as-is.
- **AC3 — Registry holds the bundle.** `WorldRegistry::new` takes a single `Arc<WorldRules>` instead of the
  ~15 individual rule args; the per-world scheduler reads `self.world_rules.<set>`.
- **AC4 — Behaviour preserved.** No domain change (P3); the bundle holds exactly today's `classic` rules, so
  every read returns the same value. The full existing suite passes unchanged; `main.rs` loads the bundle
  once.

## Design

- **`WorldRules`** (`infrastructure/src/world_rules.rs`): a plain struct of owned rule values, wrapped in one
  `Arc<WorldRules>` (one allocation per preset, shared across that preset's worlds). `load_world_rules()`
  calls the existing balance loaders and assembles it; re-exported from the infra crate.
- **`AppState`**: drop `rules`/`build_rules`/`unit_rules`/`combat_rules`/`culture_rules`/`loyalty_rules`/
  `alliance_rules`/`ranking_rules`/`achievement_catalogue`/`quest_chain`/`lifecycle_rules`/`merchant_rules`/
  `wonder_rules`/`template`; add `world_rules: Arc<WorldRules>`. Handler reads map mechanically
  (`state.rules.as_ref()` → `&state.world_rules.economy`, `state.unit_rules.as_ref()` →
  `&state.world_rules.units`, `state.template` → `&state.world_rules.starting_village`, …).
- **`WorldRegistry`**: replace the rule params/fields with `world_rules: Arc<WorldRules>`; `build_and_spawn`
  reads from it. `main.rs` + the integration harness build one `Arc<WorldRules>` and pass it both places.

## Out of scope

- Per-world variation itself (049–052): the preset loader + `worlds.rule_preset` (049), the registry serving
  per-preset bundles + the context carrying the rules + the scheduler using the world's bundle (050), the
  handler migration to `ctx.rules` (050), the admin preset selector + a real 2nd preset (051). This slice
  only *bundles*; everything stays the single `classic` set.
