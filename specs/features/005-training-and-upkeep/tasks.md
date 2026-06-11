# Feature 005 — Training & upkeep — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Training domain.** `TrainingRules` + `per_unit_time_secs` (building factor + speed),
  `can_train` gates (researched, building present/trainable, count range), `batch_cost`,
  `MAX_TRAINING_BATCH`. Unit tests (**AC3** domain side, **AC4**).
- [x] **T2 — Upkeep & starvation domain.** `production_rates`/`compute_economy` gain
  `troop_upkeep` (call sites updated); `garrison_upkeep`, `starve` (highest-upkeep-first cull,
  exact counts + tie order), `depletion_secs`. Unit tests (**AC6**, **AC7** domain side, **AC8**).

## Balance + persistence

- [x] **T3 — Balance data.** `units.toml` `[training]` factor table; `construction.toml` +=
  stable/workshop (10 levels, AC1 prerequisites); `economy.toml` population entries; loader +
  tests (**AC1**).
- [x] **T4 — Migration + training repository.** `0008_training.sql` (`village_units`,
  `training_orders` + per-building partial unique index + due index, `starvation_checks`);
  `TrainingRepository` (optimistic-settle start, garrison, claim/apply with single-tx progress,
  orphan requeue). DB tests: partial apply exactness, crash-resume, busy building (**AC2**, **AC5**).
- [x] **T5 — Starvation repository.** `StarvationRepository` (upsert/cancel/claim checks,
  snapshot-guarded `apply_starvation`). DB tests: cull applied exactly once; conflict leaves the
  check pending (**AC7** persistence side).

## Application

- [x] **T6 — Training use-cases.** `order_train` (gates → settle/debit → batch) with `TrainError`;
  `process_due_training` (claim → apply k → resync). Fake-based tests: success + every rejection
  (**AC2**, **AC3**).
- [x] **T7 — Starvation use-cases.** `sync_starvation_check` (cancel / upsert at depletion) wired
  into order_build / order_research / order_smithy_upgrade / order_train / build & training
  completions; `process_due_starvation` (re-validate → cull / reschedule / done). Tests (**AC7**,
  **AC8**).
- [x] **T8 — Scheduler.** Ticks `process_due_training` + `process_due_starvation`; startup orphan
  requeue for training. DB test via processor (**AC5** restart path).

## Web

- [x] **T9 — Troop building pages.** `GET /village/troops/{building}` + `POST /village/train`
  (PRG): researched units, cost, per-unit time, count form, active batch with remaining count +
  next-completion countdown. Integration tests (**AC9**).
- [x] **T10 — Village garrison panel.** Garrison (names, counts, total upkeep) + troop building
  links on `/village`; net crop already reflects upkeep (**AC6**, **AC9**). Integration test.

## Documentation & acceptance

- [x] **T11 — Technical docs.** rustdoc; `docs/architecture/0007-training-and-starvation.md`;
  `CLAUDE.md` current (active slice).
- [x] **T12 — End-user docs.** `docs/manual/` training & feeding-your-army guide; link from index.
- [ ] **T13 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated — completing milestone **M2**.
