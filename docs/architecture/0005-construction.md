# Construction & the build queue

**Status:** Current
**Date:** 2026-06-10 · **Slice:** 003

## Context
Players need to spend resources on timed upgrades — the core build loop. This is the first feature
where the due-event engine **mutates game state** (applies a completed build), not just no-ops (P1).

## Design
- **Ordering** (`application::order_build`): validate max level, prerequisites, and affordability
  (current resources via `compute_economy`), then in one transaction **settle** the village's
  resources, **debit** the cost, and insert a `build_orders` row due at
  `now + base ÷ (speed × mainBuildingFactor)` (P7; a higher Main Building shortens it).
- **One active order** is enforced by a **unique partial index** on `build_orders(village_id) WHERE
  status='pending'` — race-proof at the storage layer (P4), not via app checks.
- **Completion** (`application::process_due_builds`, run by the `Scheduler` each tick): atomically
  claim due orders (`FOR UPDATE SKIP LOCKED`, `(complete_at, id)` order) and apply `+1` level
  (upserting the field/building row), then mark the order done — exactly once, restart-safe (P2/AC5).
- **Capacity** now derives from Warehouse/Granary **levels** (`economy::capacities`); the base cap
  applies only until they are built.
- **Building slots:** slice 003 uses fixed slots per kind (Main Building 0, Rally Point 1,
  Warehouse 2, Granary 3) since each kind is unique. Dynamic slots / parallel queues (Roman trait)
  come later.

## Consequences
- The economy is now interactive: upgrade fields for more production, build storage for more capacity.
- Pure rules (`BuildRules`) keep costs/times/prereqs as balance data.
- Web uses a plain form POST + redirect (htmx partial-swap deferred); the live countdown is client JS
  reading the server `complete_at`.

## Links
specs/constitution.md (P1, P2, P4, P7); specs/features/003-construction/;
crates/domain/src/construction.rs; crates/infrastructure/src/repo.rs (BuildRepository);
migrations/0004_build_orders.sql.
