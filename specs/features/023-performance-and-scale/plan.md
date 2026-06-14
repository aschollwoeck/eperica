# Feature 023 — Performance & scale pass — Plan

**Spec:** ./spec.md · **Status:** Reviewed

A measurement/tuning/tooling slice — almost no domain code. The work is in **infrastructure** (seeding,
scale tests, indexes), a new **`loadtest`** binary, **benches**, and **docs**.

## Large-world seeding (infrastructure test support)

- A `seed_world(pool, players)` helper in the infra test module (or a small `pub` seeding fn behind
  `#[cfg(test)]`/a test-support path) bulk-inserts via SQL `generate_series`: N users (distinct usernames,
  non-NPC, confirmed), one village each on distinct tiles, a `village_resources` row, a couple of
  `village_buildings`, and a backlog of `events` rows due now. Bulk single-statement inserts so ~1000
  players seed in well under a test's time budget.

## Scale-regression tests (CI-gated, `#[sqlx::test]`)

- **Hot reads (AC2):** seed ~1000 players, then measure best-of-N latency of `population_board` /
  `villages_of` / `map` viewport / `player_statistics` — assert each under its budget. Cross-check no N+1
  by construction (the repo reads are already single/bounded queries; hold the line under scale).
- **Scheduler throughput (AC3):** enqueue a large backlog of due `events` (or due builds), time
  `process_due` / `process_due_builds` draining a `limit`-sized batch; assert a per-batch time ceiling and
  an events/second floor; assert deterministic same-instant ordering.
- **Concurrent claim (AC5):** spawn 2 concurrent `claim_due_*` calls over one backlog; assert the union is
  each event once, no duplicates (the `FOR UPDATE SKIP LOCKED` guarantee).

## Query/index tuning (AC4) — migration `0035`

- Run `EXPLAIN (ANALYZE)` on the hot queries under the seeded world (board, villages_of, map `villages_at`,
  reports, rate_limits, reads filtered by `world_id`/`owner_id`/coordinates). Add any missing indexes
  (e.g. covering `villages(world_id)`, coordinate lookups, `events(due_at)` ordering) and fix any N+1.
  Record before/after `EXPLAIN` in the report. Touch `db.rs` so the new migration embeds.

## Load-generation harness (AC6) — `crates/loadtest`

- A small standalone binary crate (added to the workspace) that, given a base URL + concurrency + count,
  registers accounts and drives a representative action mix (register → view village → build → map read)
  with a bounded concurrency pool, collecting per-request latency. Reports throughput (req/s) and latency
  percentiles (p50/p90/p99). Pure `tokio` + `reqwest`; **not** run in CI. A representative run is recorded
  in the report.

## Micro-benchmarks (AC7) — `crates/domain/benches`

- Criterion benches over the pure hot functions: `resolve_battle` (combat), `compute_economy`
  (economy compute-on-read), `travel_time_secs` (movement). `criterion` as a dev-dependency; a `[[bench]]`
  entry with `harness = false`. Runnable via `cargo bench -p eperica-domain`.

## Report (AC8) — `docs/architecture/0025-performance-and-scale.md`

- Documents the budgets, the scale-test + load-test + bench numbers, the `EXPLAIN`/index changes, and the
  **P5 horizontal-scale** validation (stateless web + DB-as-truth; multi-scheduler safety via SKIP LOCKED).
  Plus a `docs/` operator note on running the load tool + benches.

## Reuse / notes

- The scheduler's `claim_due_*` already uses `FOR UPDATE SKIP LOCKED` (009/later) — AC5 *validates* it; no
  new mechanism. The web tier is already stateless (P5, cookie sessions) — AC5 documents + asserts it.
- Budgets follow the existing convention (server-side hot reads target ~50 ms; end-to-end test thresholds
  are looser, best-of-N, to absorb CI jitter). Floors/ceilings are documented in the report, P7-spirit.

## Risks / decisions

- **CI determinism:** scale tests assert generous best-of-N ceilings (not tight averages) to avoid flakes
  on shared CI runners, while still catching order-of-magnitude regressions (a new full scan).
- **Seed cost:** bulk SQL inserts (not the ORM) keep the ~1000-player seed cheap enough for CI.
- **Load tool scope:** offline only; it is a capacity-planning instrument, deliberately not a CI gate (that
  is a later ops pipeline).
