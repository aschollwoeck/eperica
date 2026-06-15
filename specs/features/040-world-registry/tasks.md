# Feature 040 — World registry runtime — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Additive,
behaviour-preserving — the existing suite is the regression oracle. No pure-domain task.

## Infrastructure

- [x] **T1 — `World` += speed/radius + `all_worlds`.** Surface the world row's `speed`/`radius`; add
  `all_worlds(pool)` (ordered by `created_at`). **DB test:** `all_worlds_loads_each_with_its_config`. (AC1)

## Runtime

- [x] **T2 — Per-world scheduler registry (`main.rs`).** After the home scheduler, loop `all_worlds()` and
  spawn a scheduler for each non-home world (built from its row: speed/seed/radius/release dates + the
  shared rules + a world-scoped repo/event-store); await all on shutdown. `AppState` = the home runtime
  (unchanged). (AC2/AC3/AC4)

## Acceptance

- [x] **T3 — Per-world processing + boot.** Infra test `per_world_repos_claim_independently` (two repos,
  two worlds, each claims only its own due build, AC5). Manual boot smoke test: a 2nd world's scheduler
  starts ("started scheduler for world"); the home world still registers; world B stays empty. (AC4/AC5)
- [x] **T4 — Regression.** Full workspace suite passes **unchanged**. Spec/plan/tasks + roadmap/ADR
  cross-refs.
