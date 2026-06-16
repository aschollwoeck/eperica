# Feature 049 — `worlds.rule_preset` + name-aware loader — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md)

## Approach

Small, behaviour-preserving: add the column + the row field, make `load_world_rules` take a preset name
(classic-only), thread `"classic"` through the callers. The existing suite is the oracle (every world is
`classic`). No domain change (P3).

## Stages (each a commit; suite green before advancing)

1. **Migration + `World.rule_preset`.** `0046_world_rule_preset.sql` (default `'classic'`); `World` gains
   `rule_preset`, read in the `SELECT` mapping; touch `db.rs`. DB test: a created world is `classic`. (AC1)
2. **Name-aware loader + callers.** `load_world_rules(preset)` + `KNOWN_PRESETS`/`known_preset`; callers pass
   `"classic"`. Full suite green; spec/plan/tasks. (AC2/AC3)

## Key decisions

- **Column default does the work.** No `create_world` preset param yet (052 adds it from the form); the DB
  default makes every world `classic`, so this slice ships no behaviour change.
- **Loader matches by name now, overlays later.** 049 only needs the *seam* (`load_world_rules(name)`); the
  mechanism for a non-`classic` preset's balance (overlay on classic vs full directory) is decided in 052
  when the first one is authored, so we don't over-design before there is a second preset.

## Risk

- Trivial surface. The migration is additive with a default (no backfill); the loader change is a signature
  + a match. Compile + the full suite catch any caller miss.
