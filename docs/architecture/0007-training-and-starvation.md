# Training batches and the starvation check

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 005

## Context
Slice 005 turns the 004 unit definitions into an army economy: batches train over time, finished
units join the garrison one by one, every unit eats crop, and a dry crop store starves the army
down to a sustainable size — the natural cap (GDD §2.2). Both mechanics had to stay lazy (P1): no
tick over villages, no event per trained unit.

## Design
- **A batch is one due-event row that advances.** `training_orders` stores `started_at`,
  `per_unit_secs`, `count_total`, `count_done`, and `next_complete_at`; the `i`-th unit completes
  at `started + i × perUnit`. The processor claims rows whose next completion is due, computes
  `k = elapsed ÷ perUnit − done` in the use-case, and `apply_training` moves the garrison, the
  progress, **and a piecewise resource settle** in one transaction — the store is settled segment
  by segment so each unit's upkeep starts at its own completion instant, never retroactively
  (snapshot-guarded; a conflict releases the batch for a next-tick retry). A crash between claim
  and apply re-derives the same `k`, so no unit is lost or duplicated (AC5). One batch per troop
  building, enforced by a partial unique index like every other queue (P4).
- **Upkeep is compute-on-read.** `production_rates` takes the garrison's total crop upkeep;
  `load_economy` (and every settle path) resolves it from `village_units` × roster. Nothing is
  stored that could drift (P2).
- **Starvation is one re-validating check per village.** `starvation_checks(village_id PK,
  due_at)` is upserted at every point the store or the net rate changes (orders settle, builds and
  training complete) with the exact depletion instant (`depletion_secs`); between mutations the
  rates are constant, so the instant is exact. At fire time the handler **re-validates from live
  state**: net ≥ 0 → done; store not empty → reschedule; dry → cull. The cull
  (`domain::starve`) removes one unit of the highest-upkeep garrisoned type (ties: roster order)
  until net ≥ 0, applied via a snapshot-guarded settle in one transaction — exactly once (AC7).
  Stale or early checks are harmless by construction.
- **Deviation from Travian:** the cull is a single deterministic event at depletion, not gradual
  pacing; the sustainable end state is identical and the model needs no extra schema (spec
  Decision). Villages with no garrison never starve — the 002 floor-at-zero behavior stands (AC8).

## Consequences
- Slice 007 (movement) reuses the garrison as the source troops; combat (009) consumes the same
  per-type counts and the Smithy levels from 004.
- The scheduler now ticks five processors (events, builds, unit orders, training, starvation) and
  requeues all orphans at startup; it carries the balance rules to re-validate starvation.
- Every settle path is snapshot-guarded (004's optimistic settle), so the five concurrent queues
  cannot overwrite each other's debits.

## Links
specs/constitution.md (P1, P2, P4, P7); specs/features/005-training-and-upkeep/;
crates/domain/src/units.rs (training, starve), crates/domain/src/economy.rs (upkeep);
crates/application/src/{units.rs,starvation.rs}; crates/infrastructure/src/repo.rs
(TrainingRepository, StarvationRepository); migrations/0008_training.sql.
