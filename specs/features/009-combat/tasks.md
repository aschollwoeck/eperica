# Feature 009 — Combat resolution — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Battle domain.** `MovementKind::Attack/Raid`; `UnitSpec.siege_kind`; `combat.rs`:
  `CombatRules`/`WallProfile`, `AttackMode`, `luck_factor`, `resolve_battle` (inf/cav split, wall +
  rams, morale, luck, power-law losses), `apply_losses`, attack/defense split helpers. Unit tests:
  attack vs raid losses; wall & rams; morale; determinism (**AC3**, **AC4**, **AC5**).

## Balance & persistence

- [x] **T2 — Balance.** `combat.toml` (loss/luck/morale/base + per-tribe wall bonus/durability) +
  `combat_rules()`; `[buildings.wall]` + wall population + `BuildingKind::Wall` mappings; `siege` tag
  on siege units → `siege_kind`.
- [x] **T3 — Migration + combat repository.** `0012_combat.sql` (widen `kind` CHECK; `battle_reports`
  + indexes); `CombatRepository` (`start_attack`, `claim_due_attacks`, single-tx `apply_battle`,
  `reports_for`/`report`); narrow `claim_due_movements` to reinforce/return. DB tests: resolve reduces
  both sides once + schedules a survivor return; crash-resume; report readable by both parties
  (**AC6**, **AC7**).

## Application

- [x] **T4 — Combat use-cases.** `order_attack` (validate → travel → debit/schedule),
  `process_due_combat` (gather → resolve → apply → report → return; re-sync starvation). Fake tests:
  send success + every rejection; resolution wires the domain outcome to the repo apply (**AC1**,
  **AC2**, **AC6**).
- [x] **T5 — Scheduler.** Tick `process_due_combat`; startup orphan requeue (shared). DB test via the
  processor (**AC6** restart path).

## Web

- [x] **T6 — Attack/raid send + battle reports.** Rally Point **mode** (reinforce/attack/raid) →
  `order_attack`; `GET /reports` inbox + `GET /reports/{id}` detail (forces, losses, wall, luck,
  morale); Reports link. Integration tests (**AC8**).

## Documentation & acceptance

- [x] **T7 — Technical docs.** rustdoc; `docs/architecture/0011-combat.md`; `CLAUDE.md` active slice.
- [x] **T8 — End-user docs.** `docs/manual/` combat guide; link from index.
- [ ] **T9 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
