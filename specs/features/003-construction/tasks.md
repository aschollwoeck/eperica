# Feature 003 — Construction & build queue — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Construction domain.** `BuildingKind` += `Warehouse`/`Granary`; `BuildTarget`; `BuildRules`
  (cost, base time, MB factor, max level, prerequisites); `build_time`, `can_afford`, `debit`,
  `prerequisites_met`; extend `economy::capacities` to use Warehouse/Granary **levels**. Unit tests:
  affordability (**AC2**), prerequisites (**AC4**), MB speedup (**AC6**), speed scaling (**AC7**),
  capacity-from-levels.

## Balance + persistence

- [x] **T2 — Balance data.** `specs/balance/construction.toml` (cost + base time per target/level, MB
  factors, max levels, prerequisites, Warehouse/Granary capacity-per-level); loader → `BuildRules`.
- [x] **T3 — Migration.** `0004_build_orders.sql` (table + unique partial index `WHERE status='pending'`
  + `(status, complete_at, id)` index).
- [x] **T4 — Repository.** `start_build` (settle+debit+insert, one tx; unique index → one order),
  `claim_due_builds`, `apply_build` (upsert level, mark done), `active_build`, `village_levels`.
  Integration tests: **AC1** (debit + order), **AC3** (one order), **AC5** (apply once + restart).

## Application

- [x] **T5 — Use-cases.** `order_build` (validate max-level/prereq/affordability → start_build) with
  `BuildError`; `process_due_builds` (claim → apply). Fake-based unit tests (**AC2**, **AC4**).
- [x] **T6 — Scheduler.** Infra `Scheduler` also runs `process_due_builds` each tick (System actor, AC5).

## Web

- [x] **T7 — Village build UI.** `/village` shows level + next cost + **Order upgrade** (htmx) per
  target, **Build Warehouse/Granary** for empty slots, and active build + **live countdown**;
  `POST /village/build`; JS countdown helper; conforms to ui-style-guide (**AC8**).
- [x] **T8 — Integration test.** HTTP: order an upgrade, then `/village` shows the active build + a
  countdown deadline; ordering when one is active is rejected.

## Documentation & acceptance

- [ ] **T9 — Technical docs.** rustdoc; `docs/architecture/` note on builds as due-events that mutate
  state; update `CLAUDE.md` if needed.
- [ ] **T10 — End-user docs.** `docs/manual/buildings.md` (upgrade fields/buildings, queue, costs);
  link from the index.
- [ ] **T11 — Review & accept.** Run `eperica-reviewer` on the slice diff; fix until APPROVE; open PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests (incl. the migration-boundary guard), all gates green, both docs written,
reviewer **APPROVE**, PR merged, `spec.md`/`plan.md` **Verified**, roadmap updated — reaching **First playable**.
