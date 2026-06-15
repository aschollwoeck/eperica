# Feature 038 — World-scoped event store

**Status:** Draft
**Depends on:** 037 (players), the world row, the scheduler/event store (001/023)
**Roadmap:** M9 multi-world & administration, slice 3 of 6 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** Foundational threading. Makes `WorldId` a first-class, end-to-end identity and
world-scopes the event store so the per-world scheduler (039) only ever claims/requeues **its own**
world's events. **Single-world behaviour is unchanged**; the full suite passes unchanged.

## Problem

The event store (`scheduled_events`) and the scheduler are world-blind: `schedule` / `claim_due` /
`requeue_orphaned` operate globally. With one world that is fine, but once 039 runs **many** worlds with a
**scheduler each**, a global `requeue_orphaned` (or claim) would let one world's scheduler touch another
world's events. The store must be keyed by world before per-world schedulers can exist.

> **Scope note.** The bulk per-world work — world-scoping the ~18 `process_due_*` game-table due-claims and
> running a scheduler per world — is the substance of **039** (it is exercised by the registry there). This
> slice does the smaller, safe, prerequisite plumbing: thread `WorldId` and scope the **event store**.

## Goal

- **AC1 — `scheduled_events.world_id`.** Every event row carries its world (migration 0044: add column,
  backfill existing rows to the single world, set `NOT NULL`, FK → `worlds`). Indexed for the per-world
  claim.
- **AC2 — World-scoped event store.** `PgEventStore` carries a `WorldId`; `schedule` writes it; `claim_due`
  and `requeue_orphaned` filter `WHERE world_id = $world`. An event belonging to another world is never
  claimed or requeued by a store scoped to this world.
- **AC3 — `WorldId` threaded.** `AppState` gains `world_id: WorldId`; the event store is constructed with
  the active world. (The perf seeder + tests pass a world.)
- **AC4 — Behaviour preserved.** Single-world: scheduling, claiming, the scheduler loop, and the perf
  throughput path are unchanged (the world filter matches every row). Pure `domain` untouched (P3); the
  full existing suite passes unchanged.

## Design

- **Persistence (migration 0044).** `scheduled_events.world_id uuid` — added nullable, backfilled
  `= (SELECT id FROM worlds LIMIT 1)`, then `SET NOT NULL` + `REFERENCES worlds(id)`. Replace the
  `(status, due_at, seq)` claim index with `(world_id, status, due_at, seq)`.
- **Event store.** `PgEventStore::new(pool, world_id)`; the three statements gain the `world_id` predicate /
  column. The `EventStore` trait is unchanged (still `schedule`/`claim_due`/`mark_done`) — world scoping is
  an implementation property of the store instance, so the application layer and the `process_due` use-case
  are untouched (P3).
- **Wiring.** `main.rs` passes `world.id` to the store + `AppState`; `perf::seed_heartbeats` tags rows with
  the single world; event-store tests construct the store with a world id.

## Out of scope (slice 039)

- World-scoping the `process_due_*` game due-claims (builds/movements/combat/…); a scheduler per world; the
  world registry; resolving the player per `(user, world)` in the request path.
