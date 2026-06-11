# Combat resolution — deterministic power-law battles on the movement engine

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 009

## Context
Combat is the PvP core (GDD §9). Slice 009 adds **attack** and **raid** movements that resolve **at
arrival** into casualties + a **battle report**, deterministically given the persisted inputs and a
seed (P4/P6) so any report is re-derivable and explainable (P2). It rides on the 007 movement engine
and the 004/005 unit/garrison/reinforcement state. Loot/Cranny/catapult-damage (011), scouting (010),
and conquest (014) are deferred — 009 is the battle math, the Wall + rams, and the reports.

## Design
- **The battle math is pure domain.** `combat.rs::resolve_battle(mode, input, rules, luck)` splits
  attacker power into infantry/cavalry pools, blends the defender's `defInf`/`defCav` by the attacker's
  pool shares, adds a base defence, multiplies by `1 + wallBonus(tribe, effectiveLevel)`, dampens a
  larger attacker with **morale** `min(1, (defPop/atkPop)^e)`, perturbs by **luck**, and applies
  **power-law** casualties: an **attack** wipes the loser and bleeds the winner `(loser/winner)^k`; a
  **raid** bleeds each side `otherᵏ/(atkᵏ+defᵏ)` (both keep survivors). All constants are balance data
  (`combat.toml`), so the model is tunable without code (P7). Pure ⇒ unit-tested without a clock or DB.
- **Luck is seeded, not random.** `luck_factor(worldSeed, movementId, range)` hashes the world seed
  and the attack's movement id through SplitMix64 into `[1−L, 1+L]`. The same battle re-resolves
  identically — the foundation of exactly-once + report reproducibility (P6/P2).
- **Attack/raid are new movement kinds.** They share `troop_movements` (007) with a widened `kind`
  CHECK; `claim_due_movements` is narrowed to `reinforce`/`return` and a dedicated
  `claim_due_attacks` feeds the combat processor, so the two due-event loops never contend over a row.
  The attacker's **survivors return** via the existing `return` kind (garrison rejoin + its starvation
  re-sync) — no new return machinery.
- **Resolution is application-orchestrated, applied in one tx.** `process_due_combat` claims due
  attacks and, per battle, gathers the defender's garrison + **every reinforcement group** (each scored
  with its own tribe roster), the **Wall** level, and both populations; computes attacker pools
  (Smithy-scaled); draws luck; calls `resolve_battle`; applies the loss fractions with `apply_losses`
  (deterministic rounding) to each party; and hands a `BattleApply` to the repo. `apply_battle` then —
  in **one transaction** — subtracts the defender garrison and each reinforcement group, inserts the
  report, schedules the survivor return, and marks the attack `done`. Exactly-once: a crash before
  commit is requeued (`processing → in_transit`, shared with 007) and **re-resolved to the identical
  outcome** from the seed (P2/P6).
- **Wall + rams.** The **Wall** is a new constructable building; its per-tribe balance gives a defence
  bonus per level and a ram durability. **Rams** (siege units tagged `siege = "ram"`) sum a ram-force
  pool that razes `floor(ramPower / durability)` Wall levels *before* the bonus is read — enough force
  takes it to 0. Catapults fight as units but raze nothing (building damage is 011).
- **Reports are jsonb + scalars.** `battle_reports` stores each side's forces/losses as `unit→count`
  jsonb plus the outcome, wall before/after, and the **luck/morale** that applied (GDD §9.6). Both
  parties (and only they, P4) can read a report, joined to names + coordinates and framed from the
  viewer's side.

## Consequences
- Scouting (010) adds a `scout` kind on the same engine with a separate (non-main-battle) resolver;
  siege & loot (011) extends `apply_battle` with looted resources on the return and catapult building
  damage; conquest (014) adds administrator loyalty effects — all reuse this resolution path.
- Because luck is seeded by the movement id, a battle's outcome is fixed the instant it is sent, and a
  report can always be recomputed from persisted state — auditable and fair.
- Defender reinforcements take the **same loss fraction** as the garrison (they share the defence's
  fate) — faithful and simple; differentiated per-group survival is a later refinement.

## Links
specs/constitution.md (P1, P2, P3, P4, P6, P7); specs/features/009-combat/; specs/balance/combat.toml;
crates/domain/src/combat.rs (resolve_battle, apply_losses, luck_factor); crates/application/src/combat.rs
(order_attack, process_due_combat), crates/application/src/ports.rs (CombatRepository);
crates/infrastructure/src/repo.rs (start_attack, claim_due_attacks, apply_battle, reports),
crates/infrastructure/src/event_store.rs (scheduler tick); crates/web/src/handlers.rs (rally mode,
reports); migrations/0012_combat.sql.
