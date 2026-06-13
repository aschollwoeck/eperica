# Conquest — loyalty, administrators, and the ownership transfer

**Status:** Current
**Date:** 2026-06-13 · **Slice:** 014

## Context
Conquest is the **aggressive** multi-village path (GDD §3.3, §3.4, §6.1, §9.4 step 5): every village
carries a **loyalty** that regenerates over time; an **administrator** (Senator/Chief/Chieftain) that
survives a **won** attack lowers it, and at **zero** — with the attacker holding a **free expansion
slot** (013) and the target **not a capital** — **ownership transfers**. It rides the **009 battle
engine verbatim** (administrators are ordinary combatants) and adds only **resolution step 5** plus the
**transfer**, applied in the **same transaction** as the existing battle apply. No new movement kind, no
new scheduler tick, no new combat math.

## Design
- **Loyalty is a per-village lazy value (P1).** `villages.loyalty (smallint) + loyalty_updated_at`
  store it; `regenerate_loyalty(value, elapsed, rules, speed)` accrues toward `MAX_LOYALTY = 100`,
  clamped, speed-scaled (the 002 accrue with a ceiling) — computed on read, never ticked. A fresh or
  pre-014 village starts at 100 (the column default seeds every existing row). The `ConquestRepository`
  port (`village_loyalty` / `set_loyalty`) reads/anchors it; loyalty is **not** carried on the `Village`
  struct (it is only needed by the resolver + the web, so a dedicated port avoids churning every
  `Village` constructor — the capital exception already rides `Village.is_capital`).
- **Administrators are identified by a balance id-set, not a unit flag.** `LoyaltyRules.administrator_ids`
  (`["senator", "chief", "chieftain"]`) marks the conquerors; `administrator_count(troops, rules)` counts
  them. Administrators are ordinary `Expansion`-role units that **already fight** (`attack_power` /
  `add_defense` exclude only `Scout`/`Wild`, so the `Expansion` administrator falls through to the
  combatant path) — so AC2 needed no combat-math change, and the id-set avoids adding a `bool` to all 27
  `UnitSpec` constructors. (Settlers stay inert on offence via attack 0; their no-defence intent is a 013
  carry-over, out of 014 scope.)
- **The conquest decision is pure (P3).** `administrator_drop(surviving_admins, world_seed, movement_id,
  rules)` sums one **seeded** draw per administrator in `[drop_min, drop_max]` from the battle id (the
  009 luck discipline, P6). `conquest_outcome(loyalty_now, drop, is_capital, attacker_has_slot)` returns
  `{ new_loyalty, transferred }`: a **capital** drops nothing and never transfers; otherwise loyalty
  floors at 0 and the village transfers **only** at 0 loyalty **with** a free slot.
- **The resolver runs step 5 (`application/combat.rs`).** After the main battle + catapults, if the
  attacker **won** and a surviving administrator remains, `resolve_one` regenerates the target's loyalty
  to the battle instant, draws the drop, and checks the attacker's **free slot** — reusing
  `load_culture` (013) at the battle instant for **both** the slot gate (`used < allowed`) **and** the
  re-anchor values (the settled `cp` for each player). It produces a `LoyaltyApply`: **Reduced**
  (write the lowered loyalty) or **Conquered** (a `ConquestTransfer` with the post-conquest loyalty, both
  players' settled culture, and any surviving third-party reinforcement returns). The report records
  `loyalty_before/after` + `conquered`. A capital / no-administrator / lost battle attaches no step
  (zero overhead). `culture_rules` + `loyalty_rules` thread through `process_due_combat` (bound `A:
  AccountRepository + CultureRepository + ConquestRepository`), the scheduler, and `main`.
- **The transfer is one guarded transaction (`apply_battle`, P2).** **Conquered** re-points
  `villages.owner_id` **guarded on the loser still owning it** (a concurrent conquest wins the race ⇒
  `Conflict` ⇒ the whole apply rolls back and re-resolves, like the 012 occupy race), resets loyalty to
  `post_conquest_loyalty`, clears `is_capital`, **empties the garrison**, sends surviving third-party
  **reinforcements home** (007 returns), **cancels** the loser's pending `build_orders` / `unit_orders`
  / `training_orders` + outgoing in-transit movements for the village, and **re-anchors both** players'
  `player_culture` at the battle instant (013 — settle each at the OLD rate before the village count/rate
  moves between them). **Reduced** just writes the new loyalty. The village keeps its tile, fields,
  buildings, and stored resources.
  - **Disposition of every `village_id`-keyed dependency** (AC7 — *assets in/owned by the village pass
    with it; in-transit troops/shipments that can no longer reach a loyal village are forfeited*):
    garrison **emptied**; third-party reinforcements **sent home**; queues **cancelled**; the loser's
    **outgoing** movements cancelled, while troops **returning to** the lost village are **forfeited**
    (no loyal home — and leaving them would land the loser's army inside an enemy village); the
    village's **own troops stationed elsewhere** (`reinforcements.home_village = target`), its
    **in-flight trades**, and its **occupied oases** **follow the village to the new owner**
    (each derives ownership from the village, so no row change is needed); **both players' culture**
    re-anchored. A **capital** strike is a true no-op — the resolver attaches **no** `LoyaltyApply`, so
    `apply_battle` writes nothing (the report still records `before == after`).
- **You cannot attack/conquer your own village (P4, roles).** `order_attack` rejects any target the
  attacker owns (`dest.owner == owner`) — not just the selected home tile but any of their villages
  (013 multi-village) — so a self-attack never becomes a movement and the conquest step can never
  transfer a village to its own owner.

## Consequences
- **Alliances/diplomacy** (Embassy, confederation/war) are 015; conquest here is two-player. Allied
  reinforcement of a defender is the existing 007 mechanic (third-party stationed troops fight on
  defence; surviving ones are returned home on a conquest — usually none, since a winning Attack wipes
  the defending side). A player **can** be reduced to zero villages (no recovery here; a Palace keeps the
  capital). Re-capture penalties, loyalty buildings/celebrations, and beginner protection are later.
- The report reuses the 009 `battle_reports` rails (three nullable columns: `loyalty_before`,
  `loyalty_after`, `conquered`), so a conquest is readable in the existing inbox/detail (AC10), visible
  to both parties.
- The whole loop — loyalty regen, the seeded drop, the capital exception, the slot gate, and the
  transfer — is reproducible from persisted rows + the world seed (P2/P6); the transfer (or the
  loyalty-only change) applies exactly once with the battle, orphan-requeue safe.

## Links
specs/constitution.md (P1–P4, P6, P7, P11); specs/features/014-conquest/;
specs/balance/conquest.toml (loyalty + administrator ids), units.toml (senator/chief/chieftain);
crates/domain/src/loyalty.rs (LoyaltyRules, regenerate_loyalty, administrator_count/drop,
conquest_outcome), combat.rs (splitmix64 shared with the drop seeding);
crates/application/src/combat.rs (the resolver's step 5), culture.rs (load_culture reused), ports.rs
(ConquestRepository, LoyaltyApply, ConquestTransfer, ReinforcementReturn, BattleApply.loyalty,
report loyalty fields);
crates/infrastructure/src/repo.rs (ConquestRepository impl, apply_battle conquest branch, report
columns), balance.rs (loyalty_rules), event_store.rs (scheduler threads the rules);
crates/web/src/handlers.rs (village loyalty, report capture), state.rs (loyalty_rules), templates;
migrations/0022_loyalty.sql, 0023_conquest_report.sql.
