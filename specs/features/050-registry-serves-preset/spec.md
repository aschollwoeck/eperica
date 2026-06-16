# Feature 050 — the registry serves each world's rule preset

**Status:** Verified
**Depends on:** 048 (`WorldRules` bundle), 049 (`worlds.rule_preset` + name-aware `load_world_rules`).
**Roadmap:** Per-world configuration program — see [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) §B.
**Program note:** Make the runtime actually **read** the per-world preset. The registry resolves each world's
`rule_preset` to its own `WorldRules` bundle (cached per preset name), serves it through the request context,
and runs each world's scheduler under its own bundle. The game handlers stop reading the process-global
`AppState.world_rules` and read the **selected world's** rules from the context. Behaviour-preserving today
(only `classic` exists), but every per-world sim read is now keyed on the world's preset. No domain change (P3).

## Problem

After 049 a world *carries* a preset name, but nothing reads it: the registry, the request context, and every
handler still use the one process-global `classic` bundle (`AppState.world_rules`). So two worlds on different
presets would behave identically — the per-world configuration would have no effect. The runtime must resolve
and serve **the world's own** bundle everywhere a sim rule is read.

## Goal

- **AC1 — Registry resolves & caches per-preset bundles.** The registry holds a `preset name → Arc<WorldRules>`
  cache (seeded with the boot bundle). On first access to a world it resolves the world's `rule_preset` to its
  bundle via `load_world_rules` and caches it (one bundle per preset, shared across that preset's worlds). An
  unknown/invalid preset is logged and treated as "world not serviceable" (the request falls back to the home
  world; the scheduler refuses to start that world) — never a panic (P4).
- **AC2 — The request context carries the world's rules.** `GameContext` and `WorldScope` each expose
  `rules: Arc<WorldRules>` — the **selected** world's bundle, resolved through the registry alongside the
  world's repo/map/speed. A second-world player (or visitor) reads that world's rules.
- **AC3 — Each world's scheduler runs under its own bundle.** A world's per-world scheduler (boot + admin
  create) is built from the world's resolved bundle, not the global one — so due-event processing (training,
  combat, culture, lifecycle, …) uses the world's preset.
- **AC4 — Game handlers read rules from the context.** Every handler that reads a sim rule for the selected
  world reads it from `ctx.rules` (GameContext) or `world.rules` (WorldScope), not `AppState.world_rules`. The
  process-global `AppState.world_rules` remains only for the **non-world-scoped** paths: account creation
  (`register_submit`), world joining (`join_world`), and the cross-world account messaging pages
  (`messages`, `conversation`) — all of which act on the home/account level, not a selected world's sim.
- **AC5 — Behaviour preserved.** With only `classic` defined, every world resolves to the same balance, so the
  full suite passes unchanged. A focused test proves the context's rules track the **selected** world.

## Design

- **`registry.rs`** — replace the single `world_rules: Arc<WorldRules>` field with a
  `presets: Mutex<HashMap<String, Arc<WorldRules>>>` cache; `WorldMeta` gains `rules: Arc<WorldRules>` (and
  drops `Copy` for `Clone`). A private `rules_for(&self, preset) -> Option<Arc<WorldRules>>` returns the cached
  bundle or loads+caches it (logging + `None` on failure). `context_for` returns the world's
  `(repo, map, speed, radius, rules)`; `build_and_spawn` resolves the world's bundle and builds the map/repo/
  scheduler from it. `WorldRegistry::new` takes the boot `(preset, Arc<WorldRules>)` to seed the cache.
- **`auth.rs`** — `GameContext`/`WorldScope` gain `pub rules: Arc<WorldRules>`, populated from the registry's
  `context_for` tuple. Home-world fallback already in place is unchanged.
- **`handlers.rs`** — the 23 `GameContext` handlers + the `village_view_data` helper read `ctx.rules.*`; the 3
  `WorldScope` handlers read `world.rules.*`; the 4 non-world-scoped handlers keep `AppState.world_rules.*`.
- **`main.rs`** — pass the home world's `(rule_preset, world_rules)` to `WorldRegistry::new`; `AppState` keeps
  `world_rules` (the home bundle) for the non-world-scoped paths.

## Out of scope

- The **admin preset selector** on world creation + a real 2nd preset and its balance overlay, incl. the
  overlay-vs-full-directory decision (052). End-to-end **acceptance** that two worlds on different presets
  diverge (053).
