# Feature 049 — `worlds.rule_preset` + name-aware loader — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving —
every world is `classic`; the existing suite is the regression oracle.

- [x] **T1 — Migration + `World.rule_preset`.** `0046_world_rule_preset.sql` adds `rule_preset text NOT NULL
  DEFAULT 'classic'`; `World` gains `rule_preset` read from the `SELECT`; touch `db.rs` so the migration
  re-embeds. DB test: a created world reads `classic`. (AC1)

- [x] **T2 — Name-aware loader + callers.** `load_world_rules(preset: &str)` (classic-only; unknown ⇒
  `BalanceError`) + `KNOWN_PRESETS`/`known_preset`; `main.rs`, perf, and the test harness pass `"classic"`.
  Full suite green; spec/plan/tasks. (AC2/AC3)
