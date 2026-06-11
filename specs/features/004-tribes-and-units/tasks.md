# Feature 004 — Tribes & units — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Building kinds & catalog gates.** `BuildingKind` += `Barracks`, `Academy`, `Smithy`,
  `Stable`, `Workshop` (fix every exhaustive match: mappers, labels, slots);
  `specs/balance/construction.toml` += barracks/academy/smithy (10 levels) + prerequisites per AC5.
  Unit tests: prerequisite gates (**AC5**).
- [x] **T2 — Unit domain.** `domain/units.rs`: `UnitId`, `UnitRole`, `UnitSpec`, `ResearchSpec`,
  `UnitRules` (+ validation rules), `SmithyRules` (cost-permille/time per level), pure gates
  `can_research` / `can_upgrade`, `research_time` / `upgrade_time` (÷ speed), tier-1 =
  `research: None`. Unit tests: gates (**AC6/AC7/AC10/AC11** domain side), tier-1 (**AC9**),
  speed scaling (**AC14**).
- [x] **T3 — Lane rule.** `queue_lane(tribe, target)` — Romans get field/building lanes, others
  `all`. Unit tests (**AC13** domain side).

## Balance + persistence

- [x] **T4 — Units balance data + loader.** `specs/balance/units.toml`: 3 tribes × 10 faithful
  units (all §6.2 attributes, roles, trained_in, research cost/time/requirements) + `[smithy]`
  tables; `balance.rs` loader → `UnitRules`, **fail fast** on incomplete data. Tests (**AC4**).
- [x] **T5 — Tribe migration + registration.** `0005_tribes.sql` (users.tribe + CHECK, backfill
  Gauls, villages backfill); `RegisterCommand.tribe` validated (**AC1/AC2**); `create_account`
  stores tribe on user + village. Tests incl. migration-boundary backfill (**AC3**).
- [x] **T6 — Unit tables + repository.** `0006_units.sql` (`village_research`,
  `village_unit_levels`, `unit_orders` + partial unique indexes per kind + due index); repo:
  `start_unit_order`, `claim_due_unit_orders`, `apply_unit_order` (idempotent),
  `active_unit_orders`, `researched_units`, `unit_levels`, `requeue_orphaned_unit_orders`.
  DB tests: apply-once + restart survival (**AC8/AC12**), one-per-kind under duplicates.

## Application

- [x] **T7 — Use-cases.** `order_research` / `order_smithy_upgrade` (validate → settle/debit/insert)
  with error enums; `process_due_unit_orders`. Fake-based tests: success + every rejection reason
  leaves state untouched (**AC6/AC7/AC10/AC11**).
- [x] **T8 — Scheduler & lanes.** Scheduler ticks `process_due_unit_orders` + startup orphan
  requeue; `0007_build_lanes.sql` + `order_build` derives lane from tribe (**AC13**). DB tests:
  Roman field+building coexist; same-lane and non-Roman second order → rejected.

## Web

- [x] **T9 — Registration & village UI.** Tribe radio group on `/register` (descriptions; required;
  server-rejects unknown — **AC1**); `/village` shows tribe + Academy/Smithy links when built.
  Check ui-style-guide first; add new components to the guide if missing. Integration tests.
- [x] **T10 — Academy & Smithy UI.** `GET /village/academy` + `POST /village/academy/research`,
  `GET /village/smithy` + `POST /village/smithy/upgrade` (PRG pattern, live countdown, gate
  reasons, building-required states). Integration tests (**AC15**).

## Documentation & acceptance

- [ ] **T11 — Technical docs.** rustdoc on new public items; `docs/architecture/` note (tribes,
  unit queues, build lanes); `CLAUDE.md` current.
- [ ] **T12 — End-user docs.** `docs/manual/` — tribes (choosing, traits) + Academy/Smithy guide;
  link from index.
- [ ] **T13 — Review & accept.** Full gates + P11 check; run `eperica-reviewer` on the slice diff;
  fix until **APPROVE**; open PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC15** pass with tests (incl. the tribe-backfill migration-boundary guard), all gates green,
both docs written, reviewer **APPROVE**, PR merged, `spec.md`/`plan.md` **Verified**, roadmap updated.
