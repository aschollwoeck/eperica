# Feature 038 — World-scoped event store — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Additive, behaviour-preserving plumbing. The `EventStore` trait is unchanged — world scoping is a property
of the `PgEventStore` *instance* (it knows its world), so the application layer / `process_due` are
untouched (P3). The existing suite is the regression oracle.

## Layers

- **Persistence (migration 0044).** `scheduled_events.world_id`: add nullable → backfill to the single
  world (`SELECT id FROM worlds LIMIT 1`) → `SET NOT NULL` + `REFERENCES worlds(id)`. Swap the claim index
  to lead with `world_id`.
- **Infra (`event_store.rs`).** `PgEventStore { pool, world_id }`; `new(pool, world_id)`. `schedule` inserts
  `world_id`; `claim_due` and `requeue_orphaned` add `WHERE world_id = $world`. New event-store test: an
  event inserted for a different world is not claimed/requeued.
- **Infra (`perf.rs`).** `seed_heartbeats` tags rows with the single world (subquery).
- **Web (`state.rs`, `main.rs`).** `AppState.world_id: WorldId`; construct the store + state with `world.id`.
- **Call-site updates.** Every `PgEventStore::new` (perf main, repo/event-store tests) passes a world id.

## Key decision

- **Scope the store instance, not the trait.** Keeps the world detail in infrastructure; the pure use-cases
  and the `process_due` signature do not change. The per-world *game* due-claims (the large set) are 039.

## Risk

- The migration must backfill before `SET NOT NULL`. Guarded by the `SET NOT NULL` itself (aborts on any
  null) and a DB test. Low — `scheduled_events` holds only heartbeats (tests + perf tool) today.
