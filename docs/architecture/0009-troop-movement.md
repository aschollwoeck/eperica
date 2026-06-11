# Troop movement & travel — a due-event movement engine

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 007

## Context
Troops needed to leave a village, cross the map, and arrive after a computed delay — the non-combat
movement engine combat (009), scouting (010), trade (008), and settling (013) will all ride on. It
had to be lazy and reproducible (P1/P2): nothing polls a movement in flight; arrival is a scheduled
instant resolved authoritatively (P4); world speed scales the timing (P7). It builds directly on the
005 garrison (`village_units`) and the 006 toroidal distance.

## Design
- **Travel time is pure domain.** `movement.rs`: `travel_time_secs(distance, slowest_speed, speed)`
  = `distance ÷ (slowest × worldSpeed) × 3600`, and `_floored` clamps to ≥ 1 s (AC3). `slowest_speed`
  picks the **minimum** `speed` over the present unit types, so a slow unit paces the whole army
  (GDD §8.1). Distance is the 006 toroidal map distance; world speed multiplies — all as data (P7).
- **A movement is one advancing due-event row.** `troop_movements` (kind `reinforce`/`return`,
  origin/dest tiles, `depart_at`, `arrive_at`, `status`) with a `movement_troops` child table — the
  same pattern as the build/training/unit-order queues: claim due rows with `FOR UPDATE SKIP LOCKED`
  ordered by `(arrive_at, id)`, **apply + flip to done in one transaction** (exactly-once, P2), and
  **requeue orphans** (`in_transit` past their arrival) at scheduler startup for crash recovery (AC4).
- **Away troops leave `village_units`.** `start_reinforcement` debits the garrison in the same
  transaction that writes the movement, so the source's 005 upkeep/starvation automatically drops for
  the absent troops with **no cross-village rework** (AC6). The debit is **guarded**: because the
  `count > 0` CHECK forbids updating a row to zero, it does `UPDATE … SET count = count − n WHERE
  count > n`, else `DELETE … WHERE count = n`, requiring exactly one affected row — otherwise
  `Conflict` (a concurrent debit raced) and nothing is removed (AC2).
- **Arrival is `apply_movement`, exactly once.** Reinforce upserts into `reinforcements`
  (`host_village, home_village, unit_id` → count, `ON CONFLICT DO UPDATE … + EXCLUDED.count`); return
  upserts back into `village_units`. Stationing/rejoining and the `status='done'` flip share the one
  transaction, so a crash mid-apply re-runs cleanly via the orphan requeue.
- **Stationed troops don't yet eat at the host.** `reinforcements` is its own table keyed by
  `(host_village, home_village, unit_id)`; the host's upkeep still reads only its own
  `village_units`. Charging the host is a deferred refinement (see the spec's Out of scope).
- **Targets resolve from a coordinate, fixed at send.** `village_at(coord)` must find a village on
  **another** tile; its id is written into the movement so a later ownership change of that tile does
  not redirect troops already in flight. There is **no in-transit recall** — recall happens after
  arrival as a `return` movement (faithful, §8.3).
- **Use-cases re-sync starvation.** `order_reinforcement` validates ownership/composition/garrison/
  target, computes travel, starts the movement, then re-syncs the home's depletion check (the
  garrison shrank). `process_due_movements` returns the home villages of **return** arrivals so the
  scheduler re-syncs them (their garrison — and upkeep — grew again, AC5/AC6).

## Consequences
- Combat (009) reuses this engine: an `attack`/`raid` kind on the same `troop_movements` queue, with
  a different `apply` (resolve a battle) instead of station/rejoin.
- Travel time is bit-reproducible from `(distance, slowest_speed, worldSpeed)` — auditable, and
  testable in the pure domain without a clock.
- Because a movement's destination id is frozen at send and arrival is exactly-once, the engine is
  robust to restarts and concurrent ticks (the integration test drives a full send → station →
  return cycle through the System actor).

## Links
specs/constitution.md (P1, P2, P4, P7); specs/features/007-troop-movement/;
crates/domain/src/movement.rs (travel time, slowest speed); crates/application/src/movement.rs
(order_reinforcement / order_return / process_due_movements), crates/application/src/ports.rs
(MovementRepository); crates/infrastructure/src/repo.rs (guarded debit, claim/apply, orphan requeue),
crates/infrastructure/src/event_store.rs (scheduler tick); crates/web/src/handlers.rs (Rally Point,
village panels); migrations/0010_movements.sql.
