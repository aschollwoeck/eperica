# Feature 012 — Oases: clear & occupy — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing. This is a large slice — each phase
lands green on its own.

## Domain & balance

- [x] **T1 — Wild animals + seeded oasis garrison.** `UnitRole::Wild`; `UnitRules.wild_animals` +
  `wild_animal_roster()` (validator leaves them unconstrained); `units.toml [wild_animals]` roster;
  `oasis_garrison(world_seed, coord, animals, rules)` (pure, seeded, scaled by distance/bonus). Unit
  tests: deterministic from seed+coord; animals have attack 0 (**AC1**). Backfill `UnitRole` matches.

- [x] **T2 — Outpost building.** `BuildingKind::Outpost` threaded through every mapping (the 011-Cranny
  set); `construction.toml [buildings.outpost]`; `economy.toml` population + `outpost_capacity_per_level`;
  an `outpost_capacity(level)` accessor. Tests: rules load; Outpost buildable; capacity rises (**AC6**).

## Persistence

- [x] **T3 — Oasis tables + repository.** `0015_oases.sql`: `oases` (+ owner) + `oasis_garrison` tables;
  `troop_movements.deliver_village` nullable; `OasisAttack`/`OasisReinforce` kinds. Repo: `oasis_at`,
  `oasis_defenders` (materialised garrison or seeded animals), `occupied_oases`, `start_oasis_attack`,
  `claim_due_oasis_attacks`, single-tx `apply_oasis_battle`, `village_oasis_bonus`. DB tests: defenders
  read back; an apply writes losses + occupies once (**AC3**, **AC4**, **AC10**).

## Application — clear & occupy

- [x] **T4 — Oasis attack + clear/occupy.** `order_oasis_attack` (validate; debit; schedule);
  `process_due_oasis_combat` (gather attacker pools + oasis defenders + Outpost capacity; `resolve_battle`
  no-Wall/morale-1; apply casualties; occupy if winning + capacity; survivor return; report). Fake/DB
  tests: clear+occupy, clear-without-capacity, losing attacker (**AC2**, **AC3**, **AC4**, **AC11**).

## Application — bonus

- [x] **T5 — Production bonus.** `production_rates`/`compute_economy`/`settle_amounts` gain
  `oasis_bonus`; `village_oasis_bonus` summed and threaded through `load_economy` + every settle site
  (default-zero first, then wired). Tests: bonuses apply + stack; a holding village reads higher
  production (**AC8**).

## Application — reinforce, lose, regrow

- [ ] **T6 — Reinforce & lose.** `order_oasis_reinforce` (your oasis) + `apply_oasis_reinforce`
  (station troops); recall via `return`; `process_due_oasis_combat` fights stationed defenders →
  transfer (with capacity) or free. DB tests: reinforce stations + defends; a stronger attacker takes
  the oasis (**AC5**, **AC7**).

- [ ] **T7 — Animal regrowth.** Per-oasis regrow due-event toward the seeded strength; occupying
  cancels, freeing reschedules. DB test: a cleared unoccupied oasis regrows (**AC9**).

## Web

- [ ] **T8 — Map occupation + send/reinforce + Outpost.** Map shows oasis bonus + owner + Attack/
  Reinforce links; Rally Point routes oasis targets; village page shows occupied oases + bonus; Outpost
  buildable. Integration tests (**AC12**).

## Scheduler & acceptance

- [ ] **T9 — Scheduler + technical/end-user docs.** Tick `process_due_oasis_combat` + regrow; orphan
  requeue. rustdoc; `docs/architecture/00NN-oases.md`; `docs/manual/` oasis guide; `CLAUDE.md` active
  slice → 012.

- [ ] **T10 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
