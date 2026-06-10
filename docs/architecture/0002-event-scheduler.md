# Event scheduler (lazy time)

**Status:** Current
**Date:** 2026-06-10 · **Slice:** 001

## Context
The world must advance while players are offline without ticking every entity (P1), at sub-second
precision (P11), correctly across multiple workers (P4).

## Design
- Continuous processes (e.g. resources, later) are stored as state + timestamp and computed on read —
  never iterated on a timer.
- Discrete outcomes are rows in `scheduled_events` with a `due_at timestamptz` and a `bigserial seq`.
- `claim_due` atomically moves due rows `pending → processing` via
  `UPDATE ... WHERE id IN (SELECT ... ORDER BY (due_at, seq) FOR UPDATE SKIP LOCKED)`, so each event is
  claimed by exactly one worker and same-instant order is deterministic (P11).
- `process_due` dispatches on kind then `mark_done`. A background `Scheduler::run` poll loop drives it.

## Consequences
- Scales to many entities (only *due* events are touched) and is safe to run on multiple instances.
- Slice 001 uses a short poll interval; sleeping precisely until the next due event + `LISTEN/NOTIFY`
  wake-ups is a planned refinement for tighter P11 latency.

## Links
specs/constitution.md (P1, P11); crates/infrastructure/src/event_store.rs; migrations/0001.
