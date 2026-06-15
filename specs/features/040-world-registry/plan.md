# Feature 040 — World registry runtime — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Lowest-risk composition. The home-world setup (map, repo, the home scheduler, `AppState`) is **left exactly
as-is**; the registry is an *additive* loop that spawns a scheduler for every **other** world. So the
single-world path is untouched (the web suite is the regression oracle), and the new code only runs when a
second world exists. No domain change (P3).

## Change

- **`World` += `speed`, `radius`** (already columns) via `SELECT_COLS` + `world_from_row`; new
  `all_worlds(pool) -> Vec<World>` (ordered by `created_at`). — `crates/infrastructure/src/world.rs`.
- **`main.rs` registry loop.** After the home scheduler spawns, `all_worlds()` → for each world `≠` home:
  derive `GameSpeed` from the row (skip + log on invalid), build the per-world `WorldMap` /
  `PgAccountRepository` / `PgEventStore` (the 038/039 world-scoped instances), construct `Scheduler::new`
  from the world row's speed/seed/created_at/release dates + the shared (world-agnostic) rule `Arc`s, and
  `tokio::spawn` it with a cloned shutdown receiver. Handles are awaited on shutdown. `AppState` is the
  **home** world's runtime (unchanged).

## Why this shape

- **Home untouched ⇒ minimal blast radius.** `main.rs` is not covered by the test suite; keeping the home
  path identical means the only new behaviour is "additional worlds get a scheduler," verified by a manual
  boot smoke test (a second world's scheduler logs "started scheduler for world"; the home world keeps
  serving and registering).
- **Per-world correctness is composition.** Each extra scheduler runs on a world-scoped repo/event-store
  (038/039), so it drains only its own world — proven by the per-world claim tests.

## Tests

- Infra: `all_worlds_loads_each_with_its_config` (speed/radius per row); `per_world_repos_claim_independently`
  (two repos over two worlds each claim only their own due build).
- Manual boot smoke test: with a 2nd world row, the registry spawns its scheduler, the home world still
  registers villages, and world B stays empty (isolated).
