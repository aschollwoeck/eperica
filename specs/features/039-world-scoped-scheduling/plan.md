# Feature 039 — World-scoped due processing — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Additive, behaviour-preserving. World scoping is a property of the `PgAccountRepository` instance (it
already carries `world_id`), so only the infra claim/requeue **queries** change — the `process_due_*`
application signatures and the pure `domain` crate are untouched (P3). The existing suite — especially the
end-to-end web scheduler flows — is the regression oracle.

## Change

In `crates/infrastructure/src/repo.rs`, add `AND <village-col> IN (SELECT id FROM villages WHERE world_id =
$N)` (binding `self.world_id`) to:

- **Claims** (`status → processing`): `claim_due_builds`, `claim_due_unit_orders`, `claim_due_training`,
  `claim_due_starvation` (via `village_id`); the six `troop_movements` claims (reinforce/return, attack/raid,
  oasis-attack, oasis-reinforce, settle, scout) and `claim_due_trades` (via `home_village`).
- **Requeues** (`requeue_orphaned_*`): builds, unit orders, training, starvation, movements, trades.

The `FOR UPDATE SKIP LOCKED` + `ORDER BY <due>, id LIMIT n` are unchanged, so the claim still takes the
earliest due rows (now within the world) — determinism (P6/P11) holds.

## Not changed

- The watermark/release processors (medal settlement, inactivity sweep, wonder/artifact release) are keyed
  by their own `world_id` columns / the per-world config the scheduler already carries — already world
  correct; the registry (040) runs them per world.

## P11 budget

The added predicate is on the per-tick hot path but is **index-supported and not measurably costlier**: the
subquery `SELECT id FROM villages WHERE world_id = $N` is served by the pre-existing `UNIQUE (world_id, x, y)`
index (leads with `world_id`, migration 0001), and the driving claim is bounded by `LIMIT n`, so the
semijoin is cheap. No new index is needed. The 023 `scheduler_throughput_drains_backlog` /
`claim_takes_earliest_in_due_order` guards continue to hold, confirming no throughput/determinism regression.

## Risk

- A `$N`/bind mismatch would break a scheduler flow at runtime (not compile-time). Covered by the full
  web suite (every flow — build/train/combat/trade/move/scout/settle — exercises its claim end-to-end) plus
  a direct cross-world isolation DB test.
