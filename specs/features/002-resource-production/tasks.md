# Feature 002 — Resource production — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Integer units (`i64`) throughout.

## Domain (pure, test-first)

- [x] **T1 — Economy domain.** `economy` module: `ResourceAmounts`/`ProductionRates`/`Capacities`
  (i64), `EconomyRules` (injected balance), and `accrue`, `production_rates`, `capacities`,
  `compute_economy`. Unit tests cover **AC1** (accrual), **AC2** (speed scaling), **AC3** (cap/overflow),
  **AC4** (net crop incl. negative), **AC5** (idempotent/reproducible).

## Balance + persistence

- [x] **T2 — Balance data.** `specs/balance/economy.toml` (field production per level, field/building
  population per level, base capacity, starting amounts); infra loader → `EconomyRules` (serde DTO →
  domain). Test: loads; starting village yields positive wood/clay/iron and **positive net crop** (**AC6**).
- [x] **T3 — Migration.** `0002_village_resources.sql` (`village_id` PK, `wood/clay/iron/crop bigint`,
  `updated_at timestamptz`).
- [x] **T4 — Repository.** Extend `create_account` to seed starting amounts in the same transaction;
  add `stored_resources(village_id) -> Option<(ResourceAmounts, updated_at)>`. Integration test.

## Application + web

- [x] **T5 — Economy use-case.** `load_economy(repo, rules, speed, now, owner) -> Option<VillageEconomy>`;
  thread `EconomyRules` + `GameSpeed` through `AppState`.
- [x] **T6 — Village view.** `/village` renders the economy: per resource `amount / capacity (+rate/h)`,
  crop `/h` flagged when ≤ 0; resource color tokens + tabular figures (**AC7**, ui-style-guide).
- [x] **T7 — Integration test.** `GET /village` shows amount, capacity, and `/h` for each resource
  (extends slice 001's HTTP suite).

## Documentation & acceptance

- [ ] **T8 — Technical docs.** rustdoc on new public items; `docs/architecture/` note on the
  lazy-accrual economy; update `CLAUDE.md` if commands change (not expected).
- [ ] **T9 — End-user docs.** `docs/manual/resources.md` — how resources are produced, stored, and the
  crop/upkeep idea; link from the manual index.
- [ ] **T10 — Review & accept.** Run the `eperica-reviewer` brief on the slice diff; fix until APPROVE;
  open PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC7** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged, and
`spec.md`/`plan.md` set to **Verified** with slice 002 marked done in the roadmap.
