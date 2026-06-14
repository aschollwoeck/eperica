# Feature 023 — Performance & scale pass — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

A measurement/tuning/tooling slice, designed to be **repeatable** (one seeding library feeds the CI guard
and the on-demand tool). Each task is a commit; gates (`fmt` / `clippy -D warnings` / `test`) pass before
advancing. No gameplay changes — correctness (P2/P4/P6) is never traded for speed.

## Seeding library & scale tests

- [ ] **T1 — Reusable seeder + hot-read budgets (`infrastructure/perf.rs`; AC1/AC2).** `pub seed_world`
  (bulk-SQL, idempotent: ~N players with villages + resources + fields + buildings) and `pub seed_heartbeats`.
  Scale tests: seed ~1000 players, the population board / `villages_of` / map viewport / player-stats reads
  stay within documented budgets (best-of-N). **Tests prove AC1/AC2.**

- [ ] **T2 — Scheduler throughput + concurrent-claim safety (`infrastructure`; AC3/AC5).** With a large
  Heartbeat backlog, `process_due` drains a `limit` batch within a ceiling at/above an events/second floor,
  deterministic `due_at, seq` order; two concurrent `claim_due` calls process each event exactly once.
  **Tests prove AC3 + the scheduler half of AC5.**

## Tuning

- [ ] **T3 — Query/index audit + tuning (migration `0035`; AC4).** `EXPLAIN (ANALYZE)` the hot queries
  under the seeded world; add missing indexes / fix any N+1; re-measure. Record before/after for the report.
  **Gate:** T1/T2 scale tests stay green (and no slower).

## Repeatable tooling

- [ ] **T4 — Micro-benchmarks (`crates/domain/benches/hot.rs`; AC7).** Criterion benches for
  `resolve_battle`, `compute_economy`, `travel_time_secs`; `cargo bench -p eperica-domain` runs them.

- [ ] **T5 — `eperica-perf` tool (`crates/perf`; AC6).** A workspace binary with `seed` / `measure` / `load`
  subcommands (reusing `seed_world` + the real repos; HTTP load via `reqwest`). Re-runnable on demand
  against any `$DATABASE_URL` / server; prints fresh budget + throughput/percentile tables.

## Report & acceptance

- [ ] **T6 — Performance & scale report + review (AC8).** `docs/architecture/0025-performance-and-scale.md`
  (budgets, scale/tool/bench numbers, `EXPLAIN`/index changes, **P5** validation, **and the exact commands
  to regenerate every number**) + an operator note; `CLAUDE.md` active slice → 023. Full gates;
  `eperica-reviewer` on the slice diff; fix until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** met (scale tests green; the seeder, tool, and benches are re-runnable; indexes tuned with
evidence), all gates green, the report (with regenerate-commands) written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (023 ✅) — **M8 launch-hardening complete**.
