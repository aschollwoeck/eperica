# Feature 023 — Performance & scale pass — Plan

**Spec:** ./spec.md · **Status:** Verified

A measurement/tuning/tooling slice — almost no domain code. The work is a **reusable seeding library** +
**scale-regression tests** (infrastructure), a re-runnable **`eperica-perf`** binary, **benches**, index
**tuning**, and **docs**. The guiding constraint (this revision): **the whole pass is repeatable** — one
seeding library feeds both the CI guard and the on-demand tool, so re-running later gives comparable
numbers.

## Reusable seeding library (`crates/infrastructure/src/perf.rs`)

- `pub async fn seed_world(pool, world_id, players: u32) -> Result<SeedSummary, sqlx::Error>` — bulk SQL
  (`generate_series`, set-based inserts, `ON CONFLICT DO NOTHING` so re-runs top up rather than fail):
  N non-NPC confirmed users (`perf_…` usernames), one village each on distinct tiles, a `village_resources`
  row, 18 `village_fields` (level 1), and a few `village_buildings`. `pub async fn seed_heartbeats(pool, n)`
  bulk-inserts `n` due `scheduled_events` (Heartbeat) for the scheduler-throughput measurement.
- **One seeder, two callers:** the `#[sqlx::test]` scale tests and the `eperica-perf` tool both call it, so
  the CI guard and the on-demand pass never drift (AC1). Idempotent enough to run repeatedly on one DB.

## Scale-regression tests (CI-gated, `#[sqlx::test]`)

- **Hot reads (AC2):** seed ~1000 players, measure best-of-N latency of `population_board` / `villages_of`
  / map `villages_at` viewport / `player_statistics`; assert each under its documented budget. The repo
  reads are already single/bounded queries — these hold the line under scale (no N+1).
- **Scheduler throughput (AC3):** seed a large Heartbeat backlog; time `process_due` draining `limit`-sized
  batches; assert a per-batch ceiling + an events/second floor; assert deterministic `due_at, seq` order.
- **Concurrent claim (AC5):** two concurrent `claim_due` calls over one backlog; assert each event is
  claimed exactly once (the `FOR UPDATE SKIP LOCKED` guarantee).

## Query/index tuning (AC4) — migration `0035` *only if a gap is found*

- `EXPLAIN (ANALYZE)` the hot queries under the seeded world (board, `villages_of`/`village_by_id`, map
  `villages_at`, world/owner/coordinate filters, `scheduled_events(due_at, seq)` ordering). Add missing
  indexes; fix any N+1; re-measure. Record before/after `EXPLAIN` for the report.
- **Outcome (this pass):** the audit found the hot paths already index-backed (prior slices were
  P11-diligent) — **no migration was needed**. The only seq scan is on the tiny `users` table; the
  `population_board`'s O(villages) cost and the scheduler's per-event ack are recorded as future tuning
  targets (a naive set-based board rewrite was tried and regressed, so it was reverted). See the 0025 report.

## Repeatable perf tool (`crates/perf` → bin `eperica-perf`)

- A workspace binary (`tokio` + `sqlx` + `reqwest`), CLI via lightweight arg parsing, subcommands:
  - **`seed --players N`** — connect `$DATABASE_URL`, ensure a world, `seed_world(…, N)`; print a summary.
  - **`measure [--players N] [--heartbeats K] [--iters I]`** — optionally seed, then time the hot repo
    reads + `process_due` loop and print a budget table (path, best/median ms, pass/over budget).
  - **`load --base-url URL --concurrency C --count N`** — concurrent HTTP action mix (register → view →
    build → map) with a bounded worker pool; report req/s + p50/p90/p99.
- Reuses `eperica_infrastructure::perf::seed_world` + the real repos for `seed`/`measure`. Offline (not CI).

## Micro-benchmarks (AC7) — `crates/domain/benches/hot.rs`

- Criterion benches over the pure hot functions: `resolve_battle`, `compute_economy`, `travel_time_secs`.
  `criterion` dev-dependency + a `[[bench]] harness = false` entry. `cargo bench -p eperica-domain`.

## Report (AC8) — `docs/architecture/0025-performance-and-scale.md`

- Budgets, the scale-test + `eperica-perf` + bench numbers, the `EXPLAIN`/index changes, and the **P5**
  horizontal-scale validation (stateless web + DB-as-truth; multi-scheduler safety via SKIP LOCKED). Crucially,
  it lists **the exact commands to regenerate every number**, so the pass is repeatable. A `docs/` operator
  note covers running the tool + benches.

## Reuse / notes

- `claim_due` already uses `FOR UPDATE SKIP LOCKED`; AC5 *validates* it. The web tier is already stateless
  (cookie sessions, DB-as-truth); AC5 documents + asserts it.
- Budgets follow the existing convention (server-side hot reads ~50 ms; end-to-end test thresholds looser,
  best-of-N, to absorb CI jitter). Floors/ceilings live in the report.

## Risks / decisions

- **CI determinism:** scale tests assert generous best-of-N ceilings (catch order-of-magnitude regressions,
  not micro-jitter).
- **Seed cost / repeatability:** bulk SQL (not the ORM) + `ON CONFLICT DO NOTHING` keeps seeding cheap and
  safe to re-run on the same DB.
- **Tool, not CI gate:** `eperica-perf` and benches run on demand — the repeatable instrument — deliberately
  not wired into CI (that is a later ops pipeline).
