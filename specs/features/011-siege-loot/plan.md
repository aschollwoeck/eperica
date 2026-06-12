# Feature 011 — Siege & loot — Technical Plan

**Status:** Draft
**Spec:** ./spec.md

Two extensions to the 009 resolution and one new building. The **catapult building-damage** and the
**loot split** are pure-domain functions (symmetric with the 009 ram→Wall razing and the 008
capped-deposit). Application gathers the target's settled resources + Cranny level, runs the split,
**debits the target** and applies **building damage** in the existing `apply_battle` transaction, and
**attaches loot to the survivor `return`**; the 007 return-apply is extended to **credit** that loot
(settle + cap) on arrival. The **Cranny** is a balance/mapping addition like the 009 Wall (no schema
for the building — `village_buildings.building_type` is free text). New deps: none (loot is four
typed ints; building damage is small `jsonb`).

## Constitution check

- **P1 (event-driven):** loot is **computed-on-read** at the resolution instant (target settled via the
  002 model) and again at the **return arrival** (attacker settled before the capped credit). Nothing
  polls; both are due-event applies. Building damage is applied at resolution.
- **P2 (reproducible):** loot bundle, catapult target, and razed levels are a pure function of the
  persisted inputs + the world seed; the target debit + building damage + report + loot-bearing return
  happen in **one transaction** (009 `apply_battle`), and the credit in the return's one transaction —
  exactly-once, orphan-requeue safe. Re-resolving reproduces identical loot/damage.
- **P3 (pure domain):** `catapult_power`, `razed_levels` (shared with rams' logic), and `loot_split`
  are pure over numbers; reading the target's resources/buildings and choosing the random target are
  **application** (I/O), not domain.
- **P4 (server authority):** the client sends only `(catapult target building)`; loot amounts, the
  protection, the bypass, the damage, and the report are server-computed. The chosen target is
  re-validated (not Wall/Rally Point) server-side.
- **P6 (seeded determinism):** the random-target fallback draws from `splitmix64(world seed, movement
  id)` (the 009 luck hash), so the target — and thus the whole battle — is fixed at send and
  reproducible.
- **P7 (configurable):** `catapult_durability`, the Teuton **cranny bypass** fraction, the **Cranny
  cost/time/prereqs**, and the **per-level protection** are all balance data.
- **P11 (performance):** loot adds **one** `stored_resources` read of the target at resolution (the
  building list is already loaded for the battle); the credit reuses the return apply that already
  runs. No new hot-path queries beyond the single settle read.

## Domain (`domain`, pure)

- `BuildingKind` gains `Cranny` (exhaustive — drives the balance/repo/web mapping updates, like Wall).
- `combat.rs`:
  - `catapult_power(troops: &UnitCounts, roster, levels, rules) -> f64` — `Σ count·attack·smithy` over
    **`Siege`+`Catapult`** units (the surviving catapults; non-catapults contribute 0). Catapults stay
    in the **infantry** main-battle pool (009) — this is a *separate* sum for the damage step only.
  - `razed_levels(power: f64, durability: f64, level: u8) -> u8` — `min(level, floor(power/durability))`.
    Factor the existing **ram→Wall** razing in `resolve_battle` through this same helper (no behaviour
    change for rams).
  - `CombatRules` gains `catapult_durability: f64` and `cranny_bypass_teuton: f64` (balance).
  - `loot_split(stored: ResourceAmounts, protection: ResourceAmounts, capacity: u32) -> ResourceAmounts`
    — per type `lootable = max(0, stored − protection)`; `total = min(Σ lootable, capacity)`; split
    `total` across types **in proportion** to each `lootable` share with **round-half-to-even** (the
    009 rounding), the rounding remainder assigned to the largest lootable type so `Σ loot == total`.
    Capacity `0` or no surplus ⇒ all-zero. Pure; unit-tested (proportional, capacity-bound, cranny
    floor, conservation `loot ≤ lootable`).
  - `cranny_protection(level_capacity: i64, is_teuton: bool, bypass: f64) -> i64` — the per-type protected
    amount, reduced by the bypass fraction for a Teuton attacker (`floor(cap·(1−bypass))`).
- `carry_capacity_total(troops, roster) -> u64` helper (sum `count·carry_capacity`) for the survivors.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `construction.toml` — `[buildings.cranny]` (`time_secs`, `prerequisites = []`, `cost.{wood,clay,iron,
  crop}` per level), mirroring `[buildings.wall]`.
- `economy.toml` — `cranny_protection_per_level = [...]` (per-type protected quantity by level) +
  the Cranny **population** row; `EconomyRules` gains `cranny_protection_per_level: Vec<i64>` with a
  `cranny_protection(level)` accessor (clamped to the table).
- `combat.toml` — `catapult_durability` + `[loot] teuton_cranny_bypass = f`; `combat_rules()` loads both.
- `BuildingKind::Cranny` arm added to every mapping: `parse_building`, the `BuildRules` per-kind level
  table, the web `building_label`/`building_kind_id`/`building_slot`/`parse_building_kind`, and the
  buildable list — exactly the set the 009 Wall touched.

## Persistence (`infrastructure` + migration `0014_siege_loot.sql`)

- `ALTER TABLE troop_movements`:
  - `ADD COLUMN catapult_target text NULL` (a `BuildingKind` slug; set on attack/raid movements that
    carry catapults — mirrors 010's `scout_target`).
  - `ADD COLUMN loot_wood/loot_clay/loot_iron/loot_crop bigint NOT NULL DEFAULT 0` — the loot a
    **`return`** movement carries home (0 for reinforce/return-without-loot).
- `ALTER TABLE battle_reports`:
  - `ADD COLUMN loot_wood/clay/iron/crop bigint NOT NULL DEFAULT 0`.
  - `ADD COLUMN razed_building text NULL, razed_before smallint NULL, razed_after smallint NULL`
    (the damaged building + levels; NULL = none).
- *(No change to `village_buildings` — `building_type` is free text, so a Cranny row needs no schema.)*
- Port surface:
  - `start_attack` gains `catapult_target: Option<BuildingKind>` (written to the new column, like
    `scout_target`); `claim_due_attacks` loads it into `DueAttack.catapult_target`.
  - `BattleApply` gains `loot: ResourceAmounts`, `building_damage: Option<RazedBuilding>` (`{kind,
    before, after}`), and the **target resource debit** (`target_resources_after` + the settle
    snapshot/clock). `apply_battle` — in its existing tx — additionally: **writes the target's settled,
    looted-down resources** (snapshot-guarded, mirroring the 008 deliver write); **decrements the razed
    building's level**; **attaches the loot** to the inserted survivor `return` movement; **stores the
    report's loot + razed columns**.
  - `DueMovement`/`apply_movement` (007) **Return** arm extended: after the garrison rejoin, if the
    movement carries loot, **settle the attacker's resources** to the arrival instant and
    `deposit_capped` the loot (capped at warehouse/granary), written in the same tx — exactly once.
  - `BattleReportView` gains `loot` + `razed_building`/`razed_before`/`razed_after`; `REPORT_SELECT` +
    `report_from_row` read them.

## Application (`application`)

- `order_attack` (009) gains `catapult_target: Option<BuildingKind>`: rejected (a new `CombatError`
  arm or reuse) if it is **Wall**/**Rally Point**; persisted only when the composition holds catapults
  (else `None`), via `start_attack`.
- `process_due_combat::resolve_one` extended, **after** the main battle + survivor computation:
  1. **Catapults (AC2):** if `outcome.attacker_won` and the survivors include catapults, compute
     `catapult_power(surviving catapults)`; choose the **target building** — the chosen one if the
     target has it at level ≥ 1 and it is eligible (not Wall/Rally Point), else a **seeded-random**
     eligible building of the target (`splitmix64(world_seed, movement_id)` indexes the sorted eligible
     list); `razed = razed_levels(power, catapult_durability, level)`; record `RazedBuilding`.
  2. **Loot (AC3–AC5):** if survivors remain, read the target's `stored_resources`, settle to
     `attack.arrive_at` (002 `compute_economy`), compute per-type `cranny_protection(level, is_teuton =
     home.tribe == Teutons, bypass)`, run `loot_split(settled, protection, carry_capacity_total(
     survivors))`; the loot is **debited** from the settled amounts (→ `target_resources_after`).
  3. Assemble `BattleApply` with the loot bundle (also attached to the return), the building damage, the
     target's looted-down resource snapshot, and the extended report; hand to `apply_battle`.
  - A **scout-carrying** combined attack (010) still runs its espionage sub-step first; loot/siege are
    orthogonal and run in the same pass.
- The looted amounts feed the existing **starvation re-sync** of the target (its crop may drop).

## Interface (`web`)

- **Rally Point** send form: a **catapult target** `<select>` of building kinds (shown when catapults
  are in the composition; "(random)" default). `rally_send` parses it to `Option<BuildingKind>` and
  passes it to `order_attack`. Server re-validates (P4).
- **Battle report** detail/inbox: a **Loot** line (per-resource) and a **Building damaged** line
  (`<Building> <before> → <after>`), from the new `BattleReportView` fields.
- **Cranny** appears automatically in the build menu once it is in the balance + mappings (003 lists
  buildable kinds from `BuildRules`); its page shows the per-level protection.
- Auth via `AuthUser`; everything re-validated server-side (P4).

## Test strategy

| AC | Test |
|----|------|
| AC1 | app (fakes): an attack with catapults persists the chosen `catapult_target`; none ⇒ `None`; no catapults ⇒ `None`. |
| AC2 | domain: `razed_levels` (floor, capped); app/infra: a won battle with surviving catapults razes the chosen building, a seeded-random one when unset, none when the attacker loses / no catapult survives / target ineligible. |
| AC3 | domain: `loot_split` — proportional, capacity-bound, conservation (`Σ loot = min(Σ lootable, cap)`), all-zero when capacity 0 or no surplus. |
| AC4 | domain/app: a Cranny floor shields its per-type capacity; no Cranny ⇒ nothing shielded. |
| AC5 | domain: `cranny_protection` for a Teuton attacker is strictly lower ⇒ more lootable; non-Teuton faces full protection. |
| AC6 | infra (DB): resolution debits the target's resources once; the return carries the loot; on arrival the attacker is credited (capped at warehouse/granary); crash-resume credits exactly once. |
| AC7 | app/infra: a wiped attacker loots nothing, razes nothing, schedules no return. |
| AC8 | domain/infra: same `(inputs, seed)` ⇒ identical loot, target, razed levels. |
| AC9 | infra/web: the report persists + shows loot + building damage to both parties. |
| AC10 | app/infra (reuse 003): a Cranny can be built/upgraded; its level drives protection. |
| AC11 | web integration: send with a catapult target (PRG); the report shows loot + damage; Cranny buildable; visitor → login. |

## Notes

- **Exactly-once / conservation.** The target loses the loot in `apply_battle` (one tx, with the
  defender losses, building damage, report, and the loot-bearing return). The attacker gains it in the
  return's apply (one tx). A crash between leaves the loot **on the in-flight return row** — never lost.
  Both writes are snapshot-/done-guarded and re-run identically (P2).
- **Settle-before-debit.** The target's resources are settled to the resolution instant before
  subtracting loot (so production up to that moment counts), then written with the snapshot guard used
  by 008 deliver — a concurrent settle (trade/starvation) is detected and the resolve retried/skipped
  by the orphan requeue.
- **Catapults are double-counted by design:** they add to the infantry **attack** in the main battle
  (009) *and*, if they survive, to the **catapult** damage pool — faithful (they fight, then the
  survivors fire).
- **Wall vs catapults stay separate:** rams reduce the Wall (009); catapults never touch the Wall (it
  is excluded from the target set). A ram-razed Wall and a catapult-razed building can both appear in
  one report.
