# Feature 011 — Siege & loot — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing.

## Domain (pure, test-first)

- [x] **T1 — Siege & loot domain.** `BuildingKind::Cranny`; in `combat.rs`: `catapult_power`,
  `razed_levels` (factor the existing ram→Wall razing through it), `loot_split` (proportional,
  capacity-bound, cranny floor, round-half-to-even with conserved total), `cranny_protection`
  (Teuton bypass), `carry_capacity_total`; `CombatRules` gains `catapult_durability` +
  `cranny_bypass_teuton`. Unit tests for each (**AC2**, **AC3**, **AC4**, **AC5**, **AC8**). Backfill
  `CombatRules` literals + `BuildingKind` matches.

## Balance & the Cranny building

- [x] **T2 — Balance + Cranny mappings.** `construction.toml` `[buildings.cranny]`; `economy.toml`
  `cranny_protection_per_level` + Cranny population; `combat.toml` `catapult_durability` + `[loot]
  teuton_cranny_bypass`; loaders (`combat_rules`, economy/build rules). Thread `BuildingKind::Cranny`
  through every mapping (balance `parse_building`/level table, web label/id/slot/parse, buildable list)
  — the 009-Wall set. Tests: rules load; Cranny is buildable with positive protection (**AC10**).

## Persistence

- [x] **T3 — Migration + repository.** `0014_siege_loot.sql`: `troop_movements` gains `catapult_target`
  + `loot_{wood,clay,iron,crop}`; `battle_reports` gains `loot_*` + `razed_building/before/after`.
  `start_attack` writes `catapult_target`; `claim_due_attacks` → `DueAttack.catapult_target`.
  `BattleApply` gains loot + building damage + the target's looted-down resource snapshot; `apply_battle`
  (one tx) debits the target resources (snapshot-guarded), razes the building, attaches loot to the
  survivor `return`, writes the report fields. The 007 `return` apply credits attached loot (settle +
  `deposit_capped`). `BattleReportView` + `REPORT_SELECT`/`report_from_row` carry the new fields. DB
  tests: target debited once + building razed + loot rides the return + credited (capped) on arrival +
  crash-resume exactly once (**AC2**, **AC6**, **AC9**).

## Application

- [x] **T4 — Siege & loot use-cases.** `order_attack` accepts `catapult_target` (rejects Wall/Rally
  Point; only persisted when catapults are present). `resolve_one` extended: surviving catapults →
  target pick (chosen, else seeded-random eligible) → `razed_levels`; surviving carry capacity →
  settle target resources → `cranny_protection` (Teuton-adjusted) → `loot_split` → debit; assemble the
  loot-bearing `BattleApply`; re-sync the target's starvation. Fake tests: catapult target persisted/
  rejected; resolution wires damage + loot + no-survivor→nothing into the apply (**AC1**, **AC2**,
  **AC3**, **AC7**).

## Web

- [x] **T5 — Catapult target + loot/damage UI + Cranny.** Rally Point send gains a **catapult target**
  select (shown when catapults are in the composition; "(random)" default) → `order_attack`. Battle
  reports show **Loot** (per resource) + **Building damaged** (before → after). **Cranny** appears in
  the build menu (from balance). Integration tests (**AC11**).

## Documentation & acceptance

- [x] **T6 — Technical docs.** rustdoc; `docs/architecture/00NN-siege-loot.md` (catapult razing,
  loot split + conservation across two txs, Cranny + Teuton bypass); `CLAUDE.md` active slice → 011.
- [x] **T7 — End-user docs.** `docs/manual/` siege & loot guide (aiming catapults, raiding for
  resources, the Cranny, Teuton bypass); link from index.
- [x] **T8 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC11** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
