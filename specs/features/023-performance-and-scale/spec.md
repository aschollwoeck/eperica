# Feature 023 — Performance & scale pass

**Status:** Reviewed
**Depends on:** 021 (the full game built — every hot path now exists to measure), and the whole stack (001–022)
**Roadmap:** M8 · slice 023 · constitution **P11** (performance & timing first-class) + **P5** (horizontal scale) — the **launch-hardening capstone**: validate that the game holds its latency/throughput budgets at scale and that the architecture scales horizontally, before opening a real server.

## Goal

Every prior slice carried a P11 latency budget for its own hot path; this slice does the **system-wide
scale pass**. It (a) seeds **large worlds** and proves the hot read paths + the due-event scheduler hold
their budgets at scale, (b) **audits and tunes** the hot queries (indexes, N+1), (c) provides a standalone
**load-generation harness** + **micro-benchmarks** to measure throughput toward "thousands," and (d)
**documents** the results and the **P5 horizontal-scale** validation. No gameplay changes — measurement,
tuning, and tooling only, all **within correctness** (P2/P4/P6 are never traded for speed).

## Concepts

- **Large-world seeding.** A reusable, SQL-bulk seeder builds a world with ~1000 players (each with
  villages, resources, buildings) and a backlog of due events — fast enough to run inside a `#[sqlx::test]`
  database, so scale tests are CI-gated regression guards, not one-off experiments.
- **Hot-path budgets at scale.** The player-facing read paths (village view, map viewport, leaderboard,
  player stats) and the scheduler's claim/apply loop are measured against **documented budgets** in a
  seeded large world. Reads are **single bounded queries** (no N+1, no full scans on the hot path — P11).
- **Scheduler throughput.** `process_due*` claims a bounded batch (`limit`) and applies it; draining a
  large backlog meets a documented **events/second floor**, and same-instant ordering stays deterministic
  (P6/P11).
- **Query/index tuning.** Hot queries are **index-backed**; any missing index or N+1 found under scale is
  fixed (a migration adds indexes), with `EXPLAIN` evidence recorded in the report.
- **Horizontal scale (P5).** The web tier is **stateless** (all game state in the DB; the session is a
  signed cookie), so it scales by adding instances. The **scheduler** claims due work with
  `FOR UPDATE SKIP LOCKED`, so **multiple scheduler instances are safe** (exactly-once, no double-apply) —
  validated by a concurrent-claim test.
- **Load generation + benchmarks.** A standalone **`loadtest`** binary drives many concurrent actions
  against a running server and reports latency percentiles + throughput; **criterion** micro-benchmarks
  cover the pure hot domain functions (combat, economy, movement). Both run **offline** (not CI), with
  recorded numbers in the report.

## Acceptance criteria

> Measurement and tuning are **within correctness** (P2/P4/P6). Budgets/throughput floors are documented;
> results are reproducible from a seeded world (P2/P6).

- **AC1 — Large-world seeding.** A reusable seeder materializes a world with ≥ 1000 players (villages +
  resources + buildings) and a due-event backlog, fast enough to use in CI scale tests.

- **AC2 — Hot-read budgets at scale.** In a seeded large world, the village view, map viewport, leaderboard,
  and player-stats read paths each complete within their **documented budget** (best-of-N), and each is a
  **single bounded query** per entity (no N+1).

- **AC3 — Scheduler throughput.** With a large due-event backlog, the scheduler's claim/apply loop drains a
  full batch within a documented time and sustains a documented **events/second floor**; same-instant
  ordering remains deterministic.

- **AC4 — Query/index tuning.** Every hot-path query is index-backed under scale (no sequential scan on a
  large table on the hot path). Missing indexes are added (migration); the `EXPLAIN` evidence + the change
  list are recorded in the report.

- **AC5 — Horizontal scale (P5).** The web tier holds no per-request server state beyond the DB + the signed
  cookie, and **N concurrent scheduler instances** process each due event **exactly once** (no double-apply)
  — demonstrated by a concurrent-claim test and documented.

- **AC6 — Load-generation harness.** A standalone `loadtest` tool drives a configurable number of concurrent
  actions against a running server and reports throughput + latency percentiles; a representative run's
  numbers are recorded in the report.

- **AC7 — Micro-benchmarks.** Criterion benchmarks cover the pure hot domain functions (at least combat
  resolution, economy compute-on-read, and travel time), runnable via `cargo bench`.

- **AC8 — Report.** A performance & scale report documents the budgets, the measured numbers (scale tests +
  load test + benches), the index/query changes with `EXPLAIN` evidence, and the P5 horizontal-scale
  validation — the launch-readiness record.

## Roles & permissions

This slice adds **no player-facing surface**. The load-generation tool and the scale report are
**Administrator/operator** concerns (capacity planning, launch readiness); game behaviour is unchanged, so
the [roles.md](../../roles.md) permission matrix is untouched. All measured paths remain
server-authoritative (P4).

## Out of scope

- Application/infra **architecture changes** (sharding, read replicas, a separate job runner) — this slice
  *validates* the current horizontal-scale design (P5) and tunes within it; re-architecting is future work.
- A **continuous load-testing / perf-CI pipeline** — the harness is run on demand; wiring it into CI is a
  later ops task.
- **Client-side** performance (asset budgets, rendering) — the server command/read paths are the subject.
- Caching layers (Redis, etc.) — tuning here is queries + indexes within Postgres; external caches are
  future work if the numbers demand them.
