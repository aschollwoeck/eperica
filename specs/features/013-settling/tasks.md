# Feature 013 — Settling & culture points — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing. Large slice — each phase lands green
on its own.

## Domain & balance

- [ ] **T1 — Culture points + Town Hall.** `culture.rs`: `CultureRules` + pure `culture_rate`,
  `settle_value` (uncapped accrue), `cp_allows`, `expansion_slots`, `allowed_villages`. `culture.toml`
  + `culture_rules()` loader. `BuildingKind::TownHall` threaded through every mapping (the 012-Outpost
  set); `construction.toml [buildings.town_hall]` + `economy.toml` population. Unit tests: rate sums
  base + Town Hall; `cp_allows`/`allowed_villages` (**AC1**, **AC2**, **AC4** math).

- [ ] **T2 — Residence & Palace + settler training.** `BuildingKind::Palace` (Residence exists) threaded
  through every mapping; `construction.toml` `[buildings.residence]`/`[buildings.palace]` (Palace
  exclusive) + population. `expansion_slots_per_level` in balance. Enable the 005 settler-training gate
  (Residence **or** Palace satisfies it); confirm a settler per tribe in `units.toml`. Tests: buildings
  load; slots rise with level; settler trainable with a Residence/Palace, not without (**AC3**, **AC5**).

## Persistence — capital & culture accumulator

- [ ] **T3 — Capital flag + field cap.** `Village.is_capital` (domain field, folded into the village
  read); `construction.rs` `field_max_level(is_capital)`; balance `capital_field_max_level`. Migration
  `0018` adds `villages.is_capital`; repo `set_capital(player, village)` (one per player) wired into
  `apply_build` on a **Palace** completion. Tests: a capital field exceeds the cap, a non-capital does
  not (domain); building a Palace sets/relocates the capital (DB) (**AC9**, **AC10**).

- [ ] **T4 — Culture accumulator + rate.** Migration `0018` adds `player_culture`; `create_account`
  seeds it. Repo: `player_culture`, `settle_culture` (re-anchor), `village_town_hall_levels`; a
  `recompute_culture_rate` helper (settle-then-re-anchor) wired into `apply_build` when a **Town Hall**
  completes. DB tests: CP reads back, accrues, and re-anchors on a Town Hall change (**AC1**, **AC2**).

## Application — settle

- [ ] **T5 — Settle dispatch + found/bounce.** `troop_movements` `settle` kind (nullable deliver, 012).
  `settling.rs`: `order_settle` (own source village; free slot + settler group + Residence/Palace;
  target a free valley on another tile; debit + schedule) and `process_due_settles` / `apply_settle`
  (claim; re-validate tile-free + slot-free; **found** the 006-template village owned by the player and
  fold its CP into the rate, **or** **bounce** the settlers home; one tx, exactly-once). The slot gate
  (`village_count < allowed_villages`) is checked at **dispatch and arrival**. Fake/DB tests: found,
  bounce (taken tile / no slot), independence of the founded village, debit-once (**AC4**, **AC6**,
  **AC7**, **AC8**, **AC12**).

## Application — read

- [ ] **T6 — Culture read + web.** `load_culture` (settle-on-read: cp, rate, allowed/used slots).
  Web: a **village switcher** (every owned village; `?village=` selector defaulting to first/capital),
  the **capital** badge, a **culture panel** (CP + rate + slots used/allowed + next threshold), the
  **Settle** order on the Rally Point (offered with a free slot), Town Hall/Residence/Palace in the
  build menu, the capital's raised field cap. Integration tests (**AC11**).

## Scheduler & acceptance

- [ ] **T7 — Scheduler + technical/end-user docs.** Tick `process_due_settles`; orphan requeue (existing
  movement requeue covers `settle`). rustdoc; `docs/architecture/00NN-settling.md`; `docs/manual/`
  settling guide; `CLAUDE.md` active slice → 013.

- [ ] **T8 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
