# Feature 013 — Settling & culture points — Technical Plan

**Status:** Draft
**Spec:** ./spec.md

A player becomes **multi-village**. The new mechanics are: a **per-player culture-point accumulator**
(lazy, like resources), three new buildings (**Town Hall**, **Residence**, **Palace**), an
**expansion-slot** gate, **settler training** (enabling the 005 Residence gate), a **settle** movement
that **founds a village** on a free valley (or bounces), and the **capital** (Palace → uncapped fields
+ an unconquerable flag for 014). The battle/economy engines are reused per-village; no new external
deps.

## Constitution check

- **P1 (event-driven):** CP is **computed on read** from `(value, rate, updated_at)` — never ticked.
  The settle is a **due-event** (007 movement). Re-anchor the CP rate at each rate change.
- **P2 (reproducible):** the slot gate, the founding decision, and the capital flag derive from
  persisted rows + the world seed; the founding (or bounce) + CP re-anchor happen in **one
  transaction** (exactly-once, orphan-requeue safe).
- **P3 (pure domain):** CP math (`culture_rate`, `settle_at`, `cp_allows`), the slot rule, the settler
  group check, and the capital field-cap are pure over numbers + injected balance. Reading villages /
  founding is application I/O.
- **P4 (server authority):** the client sends only `(target tile, settlers)` / `(build Palace)`; CP,
  the slot count, tile freeness, the founding, and the capital flip are server-computed and
  **re-validated at arrival**.
- **P6 (seeded determinism):** a founded village's tile field-distribution comes from the 006 seeded
  map; the starting template + amounts are balance.
- **P7 (configurable):** CP base + per-Town-Hall rate, the CP thresholds, expansion-slots-per-level,
  settlers-per-village, the capital field cap, and the new buildings' cost/time/prereq are balance.
- **P11 (performance):** the economy read is unchanged per village; CP is one extra indexed row read.
  The multi-village pages add one `villages_of` (already used). No per-tick work.

## Domain (`domain`, pure)

- `culture.rs` (new) — `CultureRules { base_cp_per_village, town_hall_cp_per_level: Vec, cp_thresholds:
  Vec /* index = village number, [0]=0 */, expansion_slots_per_level: Vec, settlers_per_village,
  settler_id }`. Pure fns: `culture_rate(town_hall_levels: &[u8], rules) -> i64` (Σ base + TH/level);
  `settle_value(stored, rate, elapsed, …) -> i64` (the 002 `accrue` with no cap); `cp_allows(cp, rules)
  -> u32` (largest n with `threshold(n) ≤ cp`); `expansion_slots(levels: &[u8], rules) -> u32`
  (Σ per-level); `allowed_villages(cp, residence_levels) = min(cp_allows, Σ slots)`. (CP has no cap, so
  a thin accrue variant or reuse `accrue` with `i64::MAX`.)
- `building.rs` — `BuildingKind::TownHall`, `Residence` (exists), `Palace`. Threaded through every
  mapping (the 012-Outpost set).
- `construction.rs` — the field-max-level check gains a **capital** branch: `field_max_level(is_capital,
  rules)`; a capital's resource fields may reach `capital_field_max_level`. Center buildings unchanged.
- `units.rs` — `can_train` already gates settlers on `Residence`; treat **Palace** as satisfying the
  Residence requirement (a small `trains_here`/prereq adjustment, or map Palace→Residence for training).
  `UnitRole::Expansion` is already excluded from combat (verify `attack_power`/`add_defense`).

## Balance (`specs/balance/` + `infrastructure::balance`)

- `culture.toml` (new) — `base_cp_per_village`, `town_hall_cp_per_level`, `cp_thresholds`,
  `expansion_slots_per_level`, `settlers_per_village`, `settler_id`, `capital_field_max_level`. Loaded
  into `CultureRules` (a new `culture_rules()` loader, fail-fast).
- `construction.toml` — `[buildings.town_hall]`, `[buildings.residence]`, `[buildings.palace]`
  (cost/time/prereq, e.g. Town Hall ← Main Building; Residence/Palace ← Main Building at a higher
  level, Palace exclusive). `economy.toml` — population rows for the three. `BuildingKind` threaded
  through the balance/repo/web mappings.
- `units.toml` — the settler already exists (role `expansion`, `trained_in = residence`); confirm a
  settler per tribe (add the two missing tribes if only one exists).

## Persistence (`infrastructure` + migration `0018_settling.sql`)

- `player_culture(player_id PK, value bigint, rate_per_hour bigint, updated_at timestamptz)` — the lazy
  CP accumulator, one row per player, **upserted** at registration (002 pattern). Re-anchored whenever
  the rate changes.
- `villages.is_capital boolean NOT NULL DEFAULT false` — the capital flag (at most one true per owner;
  enforced in the apply, not a constraint, since "per owner" needs the owner join).
- `troop_movements` — a new `settle` kind (CHECK widened; reuse the **nullable `deliver_village`** from
  012; the settler group rides `movement_troops`, the tile is `dest_x/dest_y`).
- Repo (`PgAccountRepository`): `player_culture(player) -> (value, rate, updated_at)` +
  `settle_culture(player, value, rate, at)` (re-anchor); `village_town_hall_levels(player)` (for the
  rate); `start_settle` (guarded settler debit + `settle` movement); `claim_due_settles`;
  `apply_settle` (one tx: if the tile is free + slot free → **found** the village (insert village +
  fields + buildings + resources + re-anchor the player CP rate) else schedule a **return**; mark done);
  `set_capital(player, village)` (clear others, set one) wired into `apply_build` when a **Palace**
  completes; `village_count(player)`, `owned_villages_with_capital`. `create_account` seeds a
  `player_culture` row.

## Application (`application`)

- `culture.rs` (new use-case) — `load_culture(accounts, rules, now, player) -> CultureView { cp,
  rate, allowed_villages, used_slots }` (settle-on-read). A helper `recompute_culture_rate` re-anchors
  the player accumulator after a Town Hall change or a founding (called from the build apply + settle
  apply, in-tx via repo).
- `settling.rs` (new use-case) — `order_settle(accounts, settle_repo, starvation, …, player, target,
  troops)`: validate the player owns the **source** village, has a **free slot** + the **settler
  group** + a Residence/Palace, and the target is a **free valley** on **another tile**; debit + schedule
  the `settle`. `process_due_settles(…)`: claim, re-validate (tile still free, slot still free), then
  **found** (build the 006 template village owned by the player, fold its CP into the rate) **or**
  **bounce** (return the settlers); one tx via `apply_settle`. Mirrors the 012 oasis processor shape.
- `build.rs` (009/003) — when a **Palace** build completes, set the player's **capital** (repo
  `set_capital`) and re-anchor CP if a **Town Hall** completed (rate changed). The field-max-level
  validation passes the village's `is_capital`.
- The scheduler ticks `process_due_settles`; the orphan requeue is the existing
  `requeue_orphaned_movements` (settle is a `troop_movements` row).

## Interface (`web`)

- **Village switcher** — every page that shows "the player's village" gets a switcher/list of **all**
  owned villages (a `?village=<id>` selector defaulting to the first / the capital); the village page
  shows **that** village. The **capital** is badged.
- **Culture panel** — the village page shows **CP** (current + rate), **slots used/allowed**, and the
  next-village CP threshold.
- **Settle** — the Rally Point gains a **Settle** order (send the settler group to a tile); offered
  only with a free slot. The map already marks the player's villages; the **capital** is distinguished,
  and a free valley can be targeted for settling.
- **Build menu** — Town Hall / Residence / Palace appear (the 012-Outpost mapping set); a capital's
  resource fields show the **raised** cap.

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: `culture_rate` sums base + Town Hall; `settle_value` accrues; infra: CP reads back + re-anchors. |
| AC2 | balance/domain: Town Hall loads + raises the rate with level. |
| AC3 | balance: Residence/Palace load; `expansion_slots(level)` rises; settler training needs one. |
| AC4 | domain: `allowed_villages = min(cp_allows, slots)`; app: a founding with no free slot is rejected. |
| AC5 | app/infra: a settler batch trains at a Residence/Palace and joins the garrison (005 path). |
| AC6 | infra (DB): a due settle onto a free valley **founds** a village (own economy + CP folded), once. |
| AC7 | infra (DB): a settle onto a taken/non-valley tile, or with no slot, **bounces** the settlers home. |
| AC8 | infra (DB): the founded village has its own resources/garrison/queues, addressable by id. |
| AC9 | infra (DB): building a Palace sets the capital + clears a prior one (one per player). |
| AC10 | domain/infra: a capital field may exceed the normal cap; a non-capital field cannot. |
| AC11 | web integration: the switcher lists villages, the capital is badged, CP/slots show, settle works. |
| AC12 | infra (DB): settle dispatch debits once; found/bounce applies once; crash-resume safe. |

## Notes / open risks

- **`is_capital` on the `Village` read.** Like 012's `oasis_bonus`, the repo fills `Village.is_capital`
  on every village read so construction validation sees it without an extra call — one boolean column,
  no extra query. (Add the field to the domain `Village`, default false, set from the row.)
- **CP rate re-anchoring is the subtle part.** Any change to the set/levels of Town Halls — a Town Hall
  build completing, a village founded (adds its base + TH), a village lost (014) — must
  **settle-then-re-anchor** the player accumulator in the **same transaction** as the change, or CP
  drifts (P2). Centralise in one repo helper used by `apply_build` (Town Hall) and `apply_settle`
  (founding).
- **Multi-village blast radius.** Most per-village code already keys on `village_id` (008/009/012). The
  web is the main churn: pages that assumed "the first village" must accept a selected village. Keep the
  default = first (or capital) so single-village play is unchanged.
- **Slot race.** Two in-flight settles could both pass the dispatch check; the **arrival** re-check
  (`village_count < allowed`) is the authority — the second bounces (AC7), like the 012 capacity race.
- **Phasing** (tasks T1–T8) lets each phase land green: CP+Town Hall (T1) and the capital (T3) are
  fairly standalone; the settle movement (T4) reuses the 012 nullable-deliver + bounce; the slot gate
  (T5) ties them together; web (T6) and scheduler+docs (T7) follow; review (T8).
