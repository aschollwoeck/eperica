# Performance & scale pass — the launch-readiness record

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 023

## Context
The constitution makes performance first-class (P11) and the architecture horizontally scalable (P5); every
prior slice carried a latency budget for its own hot path. This slice is the **system-wide scale pass** —
seed large worlds, measure the hot paths + the scheduler at scale, audit the queries, and validate the P5
design — and it is **repeatable**: the same seeding library backs both the CI scale tests and the
`eperica-perf` tool, so these numbers can be regenerated any time.

## How to regenerate every number (repeatable)

```bash
CARGO="$(rustup which cargo)"; export RUSTUP_TOOLCHAIN=stable
set -a && . ./.env && set +a    # DATABASE_URL

# Hot-path latency + scheduler throughput + query plans, against $DATABASE_URL (seeds N players).
# Tip: run against an isolated throwaway DB for a clean N (the seeder ANALYZEs after loading):
"$CARGO" run -p eperica-perf -- measure --players 10000 --heartbeats 10000 --iters 5 --explain

# Just (re)seed a large world for ad-hoc experiments:
"$CARGO" run -p eperica-perf -- seed --players 5000

# HTTP load against a running server (start `cargo run -p eperica-web` first):
"$CARGO" run -p eperica-perf -- load --base-url http://127.0.0.1:8080 --concurrency 32 --count 500

# Pure hot-function micro-benchmarks:
"$CARGO" bench -p eperica-domain --bench hot

# CI regression guards (seed ~1000 players in an isolated DB and assert budgets):
"$CARGO" test -p eperica-infrastructure scale_hot_reads_within_budget scheduler_throughput_drains_backlog \
    concurrent_claim_processes_each_once
```

Re-run after any slice, on bigger hardware, or before a real launch, and compare.

## Measured numbers (reference run)

Local dev (Postgres 16 in Docker, single laptop). Treat as **relative**, not absolute — regenerate on
target hardware.

**Hot read paths** — `eperica-perf measure --players 10000` on a fresh DB (best-of-5):

| Path | Latency @ 10k | Notes |
|------|---------------|-------|
| `villages_of(player)` | ~3 ms | index scan on `villages_owner_idx` |
| `player_statistics(player)` | ~5 ms | indexed point reads |
| map viewport (961 tiles) | ~18 ms | `(world_id,x,y)` unique index, batched `unnest` |
| `population_board(world, top 100)` | **~110 ms** | set-based aggregation (was ~1.9 s — see *Findings*) |

The per-player + map reads are flat from 1k→10k (index-bound). The board was the one path that scaled with
village count; it is now set-based (below).

**Scheduler throughput** — `process_due` draining a heartbeat backlog: **~315 events/s** (≈32s for 10000),
linear and dominated by the per-event `mark_done` UPDATE. Above the regression floor; see *Findings* for the
batching opportunity.

**Micro-benchmarks** — `cargo bench -p eperica-domain --bench hot` (pure functions, no I/O):

| Function | Time |
|----------|------|
| `resolve_battle` | ~150 ns |
| `compute_economy` | ~200 ns |
| `travel_time_secs` | ~11 ns |

→ The pure game logic is **nanosecond-scale**; the latency budgets are entirely **DB-bound**, confirming
the right place to tune is queries/indexes, not CPU.

**HTTP load** — `eperica-perf load --concurrency 32 --count 500` (register → view → build → map; 2000
requests): **~750 req/s, p50 9.8 ms, p90 40 ms, p99 1.37 s**, 0 failed flows. The p99 tail is registration
write-contention (a registration is a multi-table transaction) under high concurrency, not a read-path
problem.

## Query / index audit (AC4)

`EXPLAIN (ANALYZE, BUFFERS)` via `eperica-perf measure --explain` over a 3000-player world confirms the hot
paths are **index-backed** — the prior slices were P11-diligent, so the audit found **no missing index**:

- `villages_of` → *Bitmap Index Scan on `villages_owner_idx`* (0.07 ms exec).
- population board join → *Bitmap Index Scan on `villages_world_id_x_y_key`* for the world filter; the only
  seq scan is on `users` (a tiny 46-page table — negligible).
- scheduler claim → the `scheduled_events (status, due_at, seq)` composite index serves the
  `WHERE status='pending' … ORDER BY due_at, seq` claim exactly.
- conflict/stat reads → `battle_reports (attacker_player|defender_player, occurred_at DESC)` indexes.

No missing index — the only seq scan is on the tiny `users` table. (A speculative index on the board's
per-village subqueries would be redundant: they filter `village_fields/buildings WHERE village_id = …`,
already the PK prefix.)

### Findings

1. **`population_board` — fixed (set-based aggregation).** Its per-village population was two correlated
   subqueries over `village_fields`/`village_buildings`, i.e. O(villages): ~408 ms @ 3k and **~1.9 s @ 10k**.
   It is now two grouped aggregations (`field_pop`/`bldg_pop` CTEs) joined to users — **~110 ms @ 10k, a
   ~17× win**, with identical results (the board/ranking tests are unchanged and green).

   The catch that made the *first* attempt look like a regression: bulk-seeding without `ANALYZE` leaves the
   planner with stale row estimates (it thought `villages` had ~45 rows, not 10k), so it chose a bad plan
   for the set-based query (~1.1 s @ 1k). The seeder now runs `ANALYZE` after loading — representative of a
   real autovacuumed database — and the set-based plan wins decisively. **Lesson: measure with fresh stats.**

2. **Scheduler `mark_done` is per-event** — `process_due` issues one UPDATE per processed event (~315/s,
   linear). A batched `mark_done(ids[])` (one `UPDATE … WHERE id = ANY($1)`) would multiply throughput for
   large backlogs. Out of scope here (it changes the `EventStore` contract); recorded for a future pass.

## Horizontal scale (P5) validation

- **Stateless web tier.** No per-request server state lives outside the database and the signed session
  cookie (`AppState` holds only `Arc`-shared read-only rules + the DB pool). Adding web instances behind a
  load balancer needs no shared in-process state — verified by inspection + the cookie-based session
  design.
- **Multi-instance scheduler is safe.** `claim_due` (and every typed `claim_due_*`) claims work with
  `UPDATE … WHERE id IN (SELECT … FOR UPDATE SKIP LOCKED)`, so N scheduler instances **partition** a backlog
  and never double-apply. Proven by the `concurrent_claim_processes_each_once` test: two instances claiming
  the same backlog process each event **exactly once**.
- **DB as the single source of truth (P5).** All game state is in Postgres; reproducibility (P2/P6) means a
  disputed result is recomputable. The DB is the scaling bottleneck to watch; read replicas / partitioning
  are the next lever **if** the numbers above demand it on real hardware (out of scope — this pass validates
  the current design and tunes within it).

## CI regression guards

`scale_hot_reads_within_budget` (**10 000-player** world; board/villages_of/map/stats under best-of-N ceilings),
`scheduler_throughput_drains_backlog` (drains a 2000-event backlog above a floor, each once), and
`concurrent_claim_processes_each_once` run in CI on every change — generous ceilings that catch an
order-of-magnitude regression (a new seq scan) without flaking on shared runners. They reuse
`eperica_infrastructure::perf::seed_world`, the same seeder the tool uses.

## Consequences

- The scale pass is a **standing instrument**, not a one-off: `eperica-perf` + the benches + the CI guards
  can be re-run as the game grows.
- The architecture holds its budgets at the **10 000-player** scale on modest hardware. The one read that
  scaled with village count (the population board) is now set-based (~110 ms @ 10k); the remaining scoped
  target is the scheduler batch-ack, recorded for when the numbers demand it.
