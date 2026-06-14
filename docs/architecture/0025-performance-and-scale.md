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

# Hot-path latency + scheduler throughput + query plans, against $DATABASE_URL (seeds N players):
"$CARGO" run -p eperica-perf -- measure --players 3000 --heartbeats 3000 --iters 5 --explain

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

**Hot read paths** — `eperica-perf measure --players 3000` (best-of-5):

| Path | Latency | Notes |
|------|---------|-------|
| `villages_of(player)` | ~5 ms | index scan on `villages_owner_idx` |
| `player_statistics(player)` | ~4.5 ms | indexed point reads |
| map viewport (961 tiles) | ~23 ms | `(world_id,x,y)` unique index, batched `unnest` |
| `population_board(world, top 100)` | ~408 ms | the heaviest read — see *Findings* |

**Scheduler throughput** — `process_due` draining a heartbeat backlog: **~320 events/s** (≈9.4s for 3000),
the cost dominated by the per-event `mark_done` UPDATE. Comfortably above the regression floor; see
*Findings* for the batching opportunity.

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

No migration was warranted. (A speculative index on the board's per-village subqueries would be redundant:
they filter `village_fields/buildings WHERE village_id = …`, already the PK prefix.)

### Findings & recommendations (future tuning)

1. **`population_board` is O(villages)** — its per-village population is two correlated subqueries over
   `village_fields`/`village_buildings`, so the top-100 board costs ~408 ms at 3000 players and would grow
   roughly linearly (~1.3 s at 10k). It is a secondary read (a leaderboard page, not the sub-second command
   path), so it is within tolerance for launch, but it is the **#1 optimization target**. A naive set-based
   rewrite (two grouped aggregations) was tried during this pass and **regressed** to ~1.1 s at 1000
   players (a worse plan), so the correlated form is retained; a correct optimization needs careful plan
   work (likely a `LATERAL` per-village aggregate or a materialized per-village population) and its own
   slice.
2. **Scheduler `mark_done` is per-event** — `process_due` issues one UPDATE per processed event (~320/s). A
   batched `mark_done(ids[])` (one UPDATE … WHERE id = ANY($1)) would multiply throughput for large
   backlogs. Out of scope here (it changes the `EventStore` contract); recorded for a future pass.

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

`scale_hot_reads_within_budget` (1000-player world; board/villages_of/map/stats under best-of-N ceilings),
`scheduler_throughput_drains_backlog` (drains a 2000-event backlog above a floor, each once), and
`concurrent_claim_processes_each_once` run in CI on every change — generous ceilings that catch an
order-of-magnitude regression (a new seq scan) without flaking on shared runners. They reuse
`eperica_infrastructure::perf::seed_world`, the same seeder the tool uses.

## Consequences

- The scale pass is a **standing instrument**, not a one-off: `eperica-perf` + the benches + the CI guards
  can be re-run as the game grows.
- The architecture holds its budgets at the "thousands" scale on modest hardware, with two clearly-scoped
  optimization targets (the board query, the scheduler batch-ack) recorded for when the numbers demand them.
