# Feature 040 — World registry runtime

**Status:** Draft
**Depends on:** 037 (players), 038 (world-scoped event store), 039 (world-scoped due processing)
**Roadmap:** M9 multi-world & administration, slice 5 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The runtime that lets **many worlds run concurrently**. At startup the process loads every
world and runs a **scheduler per world**, each on a repo/event-store/map scoped to that world (037–039).
The **web still operates on the home world** for gameplay; request-path world selection lands with the
player UX (042). Single-world behaviour is unchanged.

## Problem

The process spawns exactly **one** scheduler, pinned to the env-configured world's map/speed/seed
(`main.rs`). With 037–039 the data layer is world-scoped, but the *runtime* still hosts one world. To run a
second world (created by an operator in 041, played in 042) its due work must be processed by **its own**
scheduler with **its own** speed/map/seed — so the runtime must become a **registry of per-world runtimes**.

## Goal

- **AC1 — Per-world config on `World`.** The `World` row carries its own `speed` + `radius` (not just the
  env config), so each world's runtime uses its own values. `all_worlds()` loads every world.
- **AC2 — A `WorldRuntime` per world.** `{ config, map, accounts (repo), event_store }`, each scoped to its
  world (the 038/039 instances). Built from the world row.
- **AC3 — A scheduler per world.** Startup loads all worlds, builds a runtime each, and spawns a scheduler
  each (sharing the world-agnostic rules + the shutdown signal). Each scheduler drains only its world's due
  work (039) with its world's speed/seed (P6/P7). All are awaited on shutdown.
- **AC4 — Web on the home world.** `AppState` is built from the **home** world's runtime (the env-configured
  world that `ensure_world` returns); gameplay handlers are unchanged. No request-path world selection yet.
- **AC5 — Per-world processing.** Two worlds' due work is processed independently and correctly: a due order
  in world B is applied by world B's repo/scheduler, never by world A's. Behaviour preserved single-world.

## Design

- **`World` += `speed: f64`, `radius: u32`** (already columns; just surfaced), via `SELECT_COLS` +
  `world_from_row`. New `all_worlds(pool) -> Vec<World>` (ordered by `created_at`).
- **`WorldRuntime`** (web): `build_world_runtime(world, pool, map_rules, starting_amounts, beginner_secs)`
  derives `WorldConfig` from the row and constructs the per-world `WorldMap`, `PgAccountRepository`, and
  `PgEventStore`. `spawn_world_scheduler(runtime, &shared_rules, shutdown_rx)` builds `Scheduler::new` from
  the runtime + the shared rules and spawns it.
- **Startup (`main.rs`).** Load all worlds → a runtime each → spawn a scheduler each; pick the home runtime
  (matching `ensure_world`'s id) for `AppState`. The home path is otherwise unchanged.
- **No domain change (P3).** This is runtime composition + an infra read; gameplay rules untouched.

## Out of scope

- Creating/archiving worlds (041) and adding/removing runtimes **live** (no restart). This slice spawns the
  schedulers at **startup**; 041 makes creation hot.
- Request-path world selection / per-`(user, world)` player resolution (042). The web stays on the home
  world.
