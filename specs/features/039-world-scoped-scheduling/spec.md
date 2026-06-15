# Feature 039 — World-scoped due processing

**Status:** Verified
**Depends on:** 037 (players), 038 (world-scoped event store), the scheduler (001/023)
**Roadmap:** M9 multi-world & administration, slice 4 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The behaviour-preserving prerequisite for **per-world schedulers** (the registry, 040).
Makes the repo's per-tick **due-claim/requeue** queries world-scoped, so a scheduler running on a
world-scoped repo only ever processes its own world's due work. Single-world behaviour is unchanged.

## Problem

The scheduler drains due work (`process_due_*`) by claiming `pending`/`in_transit` rows
(`status → processing`) from the per-feature tables (`build_orders`, `unit_orders`, `training_orders`,
`starvation_checks`, `troop_movements`, `trade_movements`). These claims — and the startup
`requeue_orphaned_*` resets — are **world-blind**. With one world that is fine, but the registry (040) runs
a **scheduler per world**: a world-blind claim would let world A's scheduler process world B's due work
**with world A's speed/map/seed** — wrong. The claims must be world-scoped first.

## Key idea — scope the repo instance (no API change)

`PgAccountRepository` already carries its `world_id` (it is constructed per world, like the 038 event
store). So world scoping is a property of the **repo instance**: each claim/requeue filters
`<village-col> IN (SELECT id FROM villages WHERE world_id = self.world_id)`. The `process_due_*` application
signatures and the pure `domain` crate are **untouched** (P3) — exactly the 038 pattern.

## Goal

- **AC1 — World-scoped claims.** Every due-claim (`build_orders`, `unit_orders`, `training_orders`,
  `starvation_checks`, the six `troop_movements` kinds, `trade_movements`) filters to the repo's world via
  its village (`village_id` / `home_village` → `villages.world_id`).
- **AC2 — World-scoped requeues.** Every `requeue_orphaned_*` (builds, unit orders, training, starvation,
  movements, trades) resets only its world's `processing` rows.
- **AC3 — Cross-world isolation.** A due row in world B is neither claimed nor requeued by a repo scoped to
  world A. Same-world rows still claim/requeue exactly as before.
- **AC4 — Behaviour preserved.** Single-world: the world filter matches every row, so scheduling is
  unchanged. Pure `domain` untouched (P3); the full existing suite passes unchanged; the 023 scheduler
  throughput/determinism guards still hold (the claim still takes the earliest `LIMIT n` due rows, now
  within the world).

## Design

- **`event_store` already world-scoped (038).** This slice extends the same instance-scoping to the
  per-feature claim/requeue queries in `crates/infrastructure/src/repo.rs`.
- **Predicate.** Each claim's inner `SELECT` gains `AND <village-col> IN (SELECT id FROM villages WHERE
  world_id = $N)`, binding `self.world_id`; each `requeue_orphaned_*` adds the same predicate. The
  `FOR UPDATE SKIP LOCKED` + `ORDER BY <due>, id LIMIT n` are unchanged, so determinism (P6/P11) holds
  within the world.
- **Already world-keyed.** The watermark/release processors (medal settlement, inactivity sweep, wonder/
  artifact release) are keyed by their own `world_id` columns / the per-world config the scheduler already
  carries, so they are not re-scoped here (the registry, 040, runs them per world).

## Out of scope (slice 040)

- The world registry (multiple `WorldRuntime`s + a scheduler per world); resolving the player per
  `(user, world)` in the request path.
