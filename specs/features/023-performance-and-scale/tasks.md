# Feature 023 — Performance & scale pass — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

A measurement/tuning/tooling slice. Each task is a commit; gates (`fmt` / `clippy -D warnings` / `test`)
pass before advancing. No gameplay changes — correctness (P2/P4/P6) is never traded for speed.

## Seeding & scale-regression tests

- [ ] **T1 — Large-world seeding + hot-read budgets (`infrastructure`; AC1/AC2).** A bulk-SQL seeder
  (~1000 players: villages + resources + buildings + a due-event backlog). Scale tests: the population
  board / `villages_of` / map viewport / player-stats reads stay within documented budgets in the seeded
  world (best-of-N). **Tests prove AC1/AC2.**

- [ ] **T2 — Scheduler throughput + concurrent-claim safety (`infrastructure`; AC3/AC5).** With a large
  due backlog, `process_due` drains a `limit` batch within a time ceiling at/above an events/second floor,
  with deterministic same-instant ordering; two concurrent `claim_due_*` calls process each event exactly
  once (SKIP LOCKED). **Tests prove AC3 + the scheduler half of AC5.**

## Tuning

- [ ] **T3 — Query/index audit + tuning (migration `0035`; AC4).** `EXPLAIN (ANALYZE)` the hot queries
  under the seeded world; add missing indexes / fix any N+1; re-measure. Record before/after evidence for
  the report. **Gate:** the scale tests from T1/T2 stay green (and faster).

## Tooling

- [ ] **T4 — Micro-benchmarks (`crates/domain/benches`; AC7).** Criterion benches for `resolve_battle`,
  `compute_economy`, `travel_time_secs`; `cargo bench -p eperica-domain` runs them. Record representative
  numbers.

- [ ] **T5 — Load-generation harness (`crates/loadtest`; AC6).** A standalone binary: concurrency + count +
  base URL → registers accounts, drives an action mix, reports throughput + p50/p90/p99. Runs offline.
  Record a representative run.

## Report & acceptance

- [ ] **T6 — Performance & scale report + review (AC8).** `docs/architecture/0025-performance-and-scale.md`
  (budgets, scale/load/bench numbers, `EXPLAIN`/index changes, **P5 horizontal-scale** validation) +
  operator note on running the tools; `CLAUDE.md` active slice → 023. Full gates; `eperica-reviewer` on the
  slice diff; fix until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** met (scale tests green, benches + load tool runnable, indexes tuned with evidence), all gates
green, the report written, reviewer **APPROVE**, PR merged, `spec.md` / `plan.md` **Verified**, roadmap
updated (023 ✅) — **M8 launch-hardening complete**.
