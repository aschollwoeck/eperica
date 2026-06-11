# Feature 007 — Troop movement & travel — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Travel domain.** `movement.rs`: `MovementKind`, `slowest_speed`, `travel_time_secs`
  (distance ÷ (slowest × world speed), 1 s floor). Unit tests: scaling with distance / world speed
  / slowest unit; min-speed pick (**AC3**).

## Persistence

- [ ] **T2 — Migration + movement repository.** `0010_movements.sql` (`troop_movements` + due
  index, `movement_troops`, `reinforcements`); `village_at` on the account repo; `MovementRepository`
  (guarded-debit `start_reinforcement`, `start_return`, claim/apply single-tx, the view queries,
  orphan requeue). DB tests: send debits garrison + writes movement; arrival stations once; crash-
  resume; return rejoins garrison (**AC1**, **AC4**, **AC5**).

## Application

- [ ] **T3 — Movement use-cases.** `order_reinforcement`, `order_return` (validate → travel →
  start, re-sync home starvation), `process_due_movements` (claim → apply → re-sync return homes).
  Fake-based tests: send success + every rejection; away-troops lower the garrison (**AC1**, **AC2**,
  **AC6**).
- [ ] **T4 — Scheduler.** Tick `process_due_movements`; startup orphan requeue for movements. DB
  test via the processor (**AC4** restart path).

## Web

- [ ] **T5 — Rally Point page + send/return.** `GET /village/rally` send form (target + per-unit
  counts); `POST /village/rally/send`, `POST /village/rally/return` (PRG). Integration tests (**AC7**).
- [ ] **T6 — Village movement panels.** Reinforcements-here (owners), troops-abroad (send-back),
  movements-in-progress (direction + countdown); Rally Point link. Integration test (**AC7**).

## Documentation & acceptance

- [ ] **T7 — Technical docs.** rustdoc; `docs/architecture/0009-troop-movement.md`; `CLAUDE.md`
  active slice.
- [ ] **T8 — End-user docs.** `docs/manual/` movement guide; link from index.
- [ ] **T9 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC7** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
