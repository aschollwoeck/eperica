# Performance & scale tooling (operator note)

Repeatable instruments for the performance & scale pass (slice 023). Full results, budgets, and findings:
[`docs/architecture/0025-performance-and-scale.md`](../architecture/0025-performance-and-scale.md).

## Prerequisites

```bash
CARGO="$(rustup which cargo)"; export RUSTUP_TOOLCHAIN=stable
set -a && . ./.env && set +a    # DATABASE_URL (a Postgres you can write to)
```

`seed`/`measure` need only `$DATABASE_URL` (they run migrations + ensure a world). `load` needs a running
server (`cargo run -p eperica-web`). Point these at a **throwaway** database — `seed` inserts many `perf_*`
accounts.

## `eperica-perf`

```bash
# Seed N perf players (idempotent — safe to re-run / top up):
"$CARGO" run -p eperica-perf -- seed --players 5000

# Seed (optional) + time the hot reads & scheduler drain; --explain adds query plans:
"$CARGO" run -p eperica-perf -- measure --players 3000 --heartbeats 3000 --iters 5 --explain

# Concurrent HTTP load against a running server (req/s + p50/p90/p99):
"$CARGO" run -p eperica-perf -- load --base-url http://127.0.0.1:8080 --concurrency 32 --count 500
```

## Benchmarks & CI guards

```bash
# Pure hot-function micro-benchmarks:
"$CARGO" bench -p eperica-domain --bench hot

# The CI scale regression guards (isolated DB, generous budgets):
"$CARGO" test -p eperica-infrastructure \
    scale_hot_reads_within_budget scheduler_throughput_drains_backlog concurrent_claim_processes_each_once
```

The CI guards and `eperica-perf` share one seeder (`eperica_infrastructure::perf::seed_world`), so the
in-CI numbers and the on-demand numbers never drift.
