# Siege & loot — catapult damage and the carry-home plunder loop

**Status:** Current
**Date:** 2026-06-12 · **Slice:** 011

## Context
A won fight should *take something home* (GDD §9.4 steps 4 & 6). Slice 011 extends the 009 resolution
with **catapult building damage** and **loot**: surviving catapults raze a targeted building, and
surviving attackers plunder resources — bounded by **carry capacity**, shielded by the defender's
**Cranny** (which **Teutons partially bypass**). The loot **rides the survivor return** and is
**credited at home on arrival**. It adds the **Cranny** building (a balance/mapping addition like the
009 Wall — `village_buildings.building_type` is free text, so no schema for the building itself).

## Design
- **The math is pure domain.** `combat.rs` gains: `razed_levels(power, durability, level)` — whole
  levels razed, **shared** by rams→Wall (009, refactored through it) and catapults→building;
  `catapult_power` (the surviving catapults, Smithy-scaled — catapults still fight in the 009 infantry
  pool, so they *fight, then the survivors fire*); `loot_split(stored, protection, capacity)` —
  per-resource `lootable = max(0, stored − protection)`, total `min(Σ lootable, capacity)` distributed
  **in proportion** with round-half-to-even and a deterministic remainder (so `Σ loot == total` and
  `0 ≤ loot ≤ lootable`); `cranny_protection(capacity, is_teuton, bypass)` (Teuton bypass);
  `carry_capacity_total`. All constants — `catapult_durability`, the Teuton `cranny_bypass`, the Cranny
  `protection_per_level` — are balance (`combat.toml`/`CombatRules`), tunable without code (P7).
- **Application orchestrates, reading persisted state.** In `process_due_combat::resolve_one`, after the
  main battle: if the attacker prevails and catapults survive, `pick_razed_target` chooses the aimed
  building (if the target has it at level ≥ 1, excluding Wall/Rally Point) **else a seeded-random**
  eligible building via the **009 luck hash** (`luck_factor(world_seed, movement_id)`) — so the target,
  and the whole battle, is fixed at send and reproducible (P6). `compute_loot` settles the target's
  resources to the arrival instant (002 model, P1), subtracts the Teuton-adjusted Cranny protection,
  and bounds by the survivors' carry capacity — the **one** extra read per battle (and only for a
  Resources-relevant loot). The catapult target rides the movement row (`catapult_target`), mirroring
  010's `scout_target`.
- **Conservation across two transactions.** `apply_battle` (one tx, with the 009 casualties + report)
  additionally: **debits** the target's settled, looted-down resources (snapshot-guarded, the 008
  deliver pattern — a concurrent settle is detected as `Conflict` and the resolve requeued/re-run),
  **decrements** the razed building's level, **attaches** the loot bundle to the survivor `return`
  movement, and records `loot` + `razed` on the report. The loot is then **credited** to the attacker in
  the **return's** apply: `process_due_movements` (now given the accounts + economy rules) settles the
  home and `deposit_capped`s the loot (overflow lost, like 008), written guarded in `apply_movement`'s
  one tx. The target loses the loot **once** at resolution; the attacker gains it **once** at arrival; a
  crash between leaves the loot **on the in-flight return row** — never lost (P2).
- **Catapults are double-counted by design:** they add to the infantry **attack** in the main battle
  (009) *and*, if they survive, to the **catapult** damage pool — faithful (fight, then fire). Rams
  still raze only the **Wall**; catapults never touch it (the Wall and Rally Point are excluded from the
  target set).
- **Cranny** is a new `BuildingKind::Cranny` threaded through the balance/repo/web mappings (the 009-Wall
  set) with a per-level protection table and population; its protection lives on `CombatRules` (loot is
  a combat read) to avoid churning every `EconomyRules` literal.

## Consequences
- Conquest (014) extends `apply_battle` again (administrator loyalty); oasis loot (012) reuses the loot
  split. The **report** rails (loot + razed) feed ranking (016 "top raiders").
- A raid now both bleeds and plunders; a Cranny is the first real defence against the latter, and the
  Teuton bypass makes the raiding tribe live up to its identity.
- Because the random target and all loot math derive from the world seed + movement id, a battle's
  loot, target, and damage are auditable and reproducible from persisted state (P2/P6).

## Links
specs/constitution.md (P1, P2, P3, P4, P6, P7); specs/features/011-siege-loot/; specs/balance/combat.toml
([loot], catapult_durability), construction.toml ([buildings.cranny]), economy.toml (cranny population);
crates/domain/src/combat.rs (razed_levels, catapult_power, loot_split, cranny_protection),
crates/domain/src/building.rs (Cranny); crates/application/src/combat.rs (pick_razed_target, compute_loot,
resolve_one), crates/application/src/movement.rs (process_due_movements loot credit),
crates/application/src/ports.rs (RazedBuilding, ResourceWrite); crates/infrastructure/src/repo.rs
(apply_battle debit/raze/loot, apply_movement credit); crates/web/src/handlers.rs (catapult target, report
loot/damage, Cranny build); migrations/0014_siege_loot.sql.
