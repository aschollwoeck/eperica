# Feature 003 — Construction & build queue — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

Builds on slices 001/002. No new external deps expected. This is the first slice where the **due-event
engine drives a real game-state change** (applying a completed build).

## Constitution check

- **P1 (lazy/event-driven):** a build is a **due-timestamped row** (`build_orders`) applied only when
  due — not polled per entity. Resource accrual stays compute-on-read; ordering a build is the mutating
  command that **settles** resources (002 decision).
- **P2 (reproducible):** the village's level state + resources are fully persisted; a pending order
  survives restart and applies once.
- **P3 (pure domain):** costs, build time, prerequisites, max level, capacity-from-levels are pure
  functions over injected `BuildRules`; the repo only persists.
- **P4 (server authority):** the client sends only "upgrade slot X"; cost/level/time/one-order are
  enforced server-side. A unique partial index guarantees one active order even under races.
- **P7 (speed):** `buildTime = base ÷ (speed × mainBuildingFactor)`.
- **P11 (performance):** ordering = a few indexed writes in one tx; the build processor claims due rows
  with `FOR UPDATE SKIP LOCKED` ordered by `(complete_at, id)`.

## Domain (`domain`, pure)

- Extend `BuildingKind` with **`Warehouse`, `Granary`** (mappers updated — exhaustive matches catch it).
- `BuildTarget` — addresses what is being built:
  `Field { slot: u8 }` | `Building { slot: u8, kind: BuildingKind }`.
- `BuildRules` (injected balance):
  - `cost(target_kind, next_level) -> ResourceAmounts`
  - `base_time_secs(target_kind, next_level) -> i64`
  - `main_building_factor(mb_level) -> f64` (or an integer permille) — higher MB ⇒ smaller factor
  - `max_level(target_kind) -> u8` (10)
  - `prerequisites(building_kind) -> &[(BuildingKind, u8)]`
- Functions: `build_time(base_secs, mb_level, speed) -> i64`; `can_afford(amounts, cost)`;
  `prerequisites_met(kind, &buildings, rules)`; `debit(amounts, cost) -> amounts`.
- **Capacity from levels:** extend `economy::capacities` to read Warehouse/Granary levels (base when
  absent). Unit tests for cost/time/MB-speedup/speed-scaling/affordability/capacity.

## Persistence (`infrastructure` + migration `0004_build_orders.sql`)

```
build_orders(
  id uuid PK, village_id uuid FK->villages ON DELETE CASCADE,
  target_table text,           -- 'field' | 'building'
  slot smallint,
  building_type text NULL,      -- for building targets / new construction
  resource_type text NULL,      -- informational for field targets
  target_level smallint,
  complete_at timestamptz, status text DEFAULT 'pending', created_at timestamptz)
CREATE UNIQUE INDEX one_active_build ON build_orders(village_id) WHERE status='pending';  -- one order (P4/AC3)
CREATE INDEX build_orders_due ON build_orders(status, complete_at, id);                   -- claim order (P11)
```

## Application (ports + use-cases)

- Port `BuildRepository` (or extend `AccountRepository`):
  - `start_build(village_id, settled: ResourceAmounts, now, order: NewBuildOrder) -> Result<(),RepoError>`
    — one tx: `UPDATE village_resources` to settled-minus-cost amounts + `updated_at=now`, then INSERT
    the order; the unique index turns a second active order into `RepoError::Duplicate`.
  - `claim_due_builds(now, limit) -> Vec<DueBuild>` (atomic `pending→processing`).
  - `apply_build(due) -> Result<(),RepoError>` — upsert the field/building row to `target_level`,
    mark the order `done` (idempotent).
  - `active_build(village_id)` + `village_levels(...)` for the view.
- Use-cases:
  - `order_build(repo, build_rules, economy_rules, speed, now, owner, target)` — load economy (current
    amounts) + village levels, validate (max level, prerequisites, affordability), compute cost+time
    (domain), then `start_build`. Maps errors to `BuildError {Insufficient, AlreadyBuilding,
    MaxLevel, PrereqUnmet, NotFound}`.
  - `process_due_builds(repo, now)` — claim due builds, `apply_build` each (the System actor, AC5).
- The infra `Scheduler` also ticks `process_due_builds` each loop (alongside heartbeats).

## Interface (`web`)

- `/village` shows each field/building: level, next-level **cost**, and an **Order upgrade** button
  (htmx POST) when idle + affordable; plus empty center slots offering **Build Warehouse/Granary**.
  While an order is active: "Building <target> → level N" with a **live countdown** to `complete_at`.
- `POST /village/build` (htmx) — body identifies the target (table+slot, and building kind for new
  builds); calls `order_build`; returns the refreshed panel (or an error message).
- **JS countdown helper** (deferred from 002) ticks from the server `complete_at` (P1/P11); htmx
  swaps the build panel on order and can refresh on completion.

## Balance data (`specs/balance/construction.toml`)

Cost + base-time per target kind & level, Main-Building speed factors per level, max levels,
prerequisites, and Warehouse/Granary capacity-per-level. Loaded into `BuildRules` (serde DTO → domain).

## Test strategy

| AC | Test |
|----|------|
| AC1 | infra/app: order debits cost + creates an order with the right `complete_at`. |
| AC2 | app/domain: insufficient resources → rejected, nothing written. |
| AC3 | infra: second concurrent/sequential order rejected (unique index). |
| AC4 | domain: prerequisites gate. |
| AC5 | infra: due build applies +1 level exactly once; pending survives a fresh processor (restart). |
| AC6 | domain: build time strictly decreases as MB level rises. |
| AC7 | domain: build time scales inversely with speed. |
| AC8 | web integration: `/village` shows cost + order action; after ordering, shows active build + a countdown deadline. |

## Notes

- New-building construction = an order targeting an empty building slot with `target_level = 1` and a
  chosen `building_type`; `apply_build` inserts the row at level 1.
- Capacity now derives from Warehouse/Granary levels, so the 002 base cap only applies until built.
