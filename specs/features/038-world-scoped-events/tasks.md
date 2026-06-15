# Feature 038 — World-scoped event store — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Additive,
behaviour-preserving — the existing suite must pass unchanged. No pure-domain task.

## Persistence

- [x] **T1 — `scheduled_events.world_id` (migration 0044).** Add nullable → backfill to the single world →
  `SET NOT NULL` + FK; swap the claim index to lead with `world_id`. (AC1)

## Infrastructure

- [x] **T2 — World-scoped `PgEventStore`.** `new(pool, world_id)`; `schedule` writes `world_id`;
  `claim_due` + `requeue_orphaned` filter by world. Update `perf::seed_heartbeats` + every `new` call site
  (perf main, tests). **DB test:** an event for world B is not claimed/requeued by a store scoped to world
  A; same-world events still claim. (AC2)

## Web wiring

- [x] **T3 — `AppState.world_id` + startup.** Add `world_id: WorldId`; construct the event store + state
  with `world.id`. (AC3)

## Acceptance

- [x] **T4 — Regression.** Full workspace suite passes **unchanged** (AC4). Spec/plan/tasks + roadmap/ADR
  cross-refs.
