# Feature 009 — Combat resolution

**Status:** Draft
**Depends on:** 004 (units + Smithy levels), 005 (garrison + reinforcements), 006 (distance), 007 (movement engine)
**Roadmap:** M4 · slice 009 · GDD §9 — the PvP core; combat rides on the movement engine.

## Goal

Troops can **attack** or **raid** another player's village. The movement travels (007), and **at the
instant of arrival** the server resolves a battle **deterministically** (P4/P6): attacker power vs
defender power with the **infantry/cavalry split**, the **Wall** bonus (reduced by **rams**),
**morale**, and **seeded luck**; **power-law casualties** fall on both sides; the **survivors return
home**; and a **battle report** is emitted to both parties. This is the heart of the game — the rails
scouting (010), siege & loot (011), and conquest (014) extend.

## Concepts

- **Attack / Raid:** two movement kinds on the 007 engine. An **attack** fights to destroy (the loser
  loses *all* participating troops); a **raid** fights to plunder (both sides take *proportional*
  losses and survivors remain). Both resolve at arrival; survivors travel home.
- **Battle power:** the attacker's units sum to an **infantry-attack** pool and a **cavalry-attack**
  pool (each unit's `attack` scaled by its Smithy level). The defender's troops (the target's garrison
  **plus** every reinforcement stationed there, 005/007) each contribute `defenseInfantry` against the
  attacker's infantry share and `defenseCavalry` against its cavalry share; plus a small village **base
  defense** and the **Wall** bonus.
- **Wall & rams:** the Wall multiplies the defender's total defense by `1 + bonus(tribe, level)`.
  **Rams** (siege) reduce the **effective Wall level** *before* the bonus is computed — heavier ram
  force razes more wall.
- **Morale:** a population-ratio dampener that weakens a much larger attacker against a much smaller
  defender (protects newer players), applied to attacker power.
- **Luck:** a bounded random factor (default ±25%) drawn from a **seeded RNG** keyed by the world seed
  and the movement id — so a battle is reproducible and explainable (P6/P2).
- **Battle report:** a persisted record for both parties — forces, losses on each side, the wall
  level razed, and the **luck and morale** that applied (GDD §9.6).

## User stories

- As a **player**, I want to attack an enemy village and destroy its troops.
- As a **player**, I want to raid for a quick fight where my survivors come home.
- As a **player**, I want a battle report explaining what happened — forces, losses, luck, morale.
- As a **defender**, I want my garrison and the reinforcements helping me to defend automatically.

## Acceptance criteria

> All combat is server-authoritative (P4) and deterministic given the persisted inputs + seed (P6):
> the troops sent, the target, travel time, the resolution, casualties, and the report are computed
> server-side; the client only issues the command.

- **AC1 — Send an attack/raid.** Given the player's village with a garrison and a target (another
  existing village on a different tile, **not the player's own**), when the player sends a chosen
  subset of their troops as an **attack** or **raid**, those troops **leave the garrison** and a
  movement of that kind is created arriving at `now + travelTime` (007 formula, paced by the slowest
  unit, P7).

- **AC2 — Send rejected.** Rejected with **nothing removed** when: a requested count exceeds the
  garrison; the composition is empty; the target tile holds no village; the target is the sender's own
  village. (Only the owner's own garrison can be sent, P4.)

- **AC3 — Deterministic resolution (P6/P2).** A battle's outcome is a pure function of the persisted
  inputs (both sides' troops + levels, Wall, populations) and the **seeded luck**; re-resolving the
  same inputs yields the **same** casualties. Luck is bounded (within the configured range) and drawn
  from the world seed + movement id — never wall-clock or online state.

- **AC4 — Battle formula.** Attacker power (infantry + cavalry pools, Smithy-scaled) is compared to
  defender power (each defender's `defInf`/`defCav` blended by the attacker's infantry/cavalry share,
  plus base defense, all × the Wall multiplier), after **morale** (dampens a much larger attacker) and
  **luck**. Casualties follow a **power-law**: in an **attack** the loser loses **all** participating
  troops and the winner loses a power-ratio fraction; in a **raid both** sides lose a proportional
  fraction (the stronger side loses less) and both keep survivors.

- **AC5 — Wall & rams.** The Wall adds a defense percentage that scales with its level (and tribe).
  **Rams** in the attack reduce the **effective Wall level** (so its bonus) before defense is summed;
  enough ram force razes the Wall to level 0. A village with no Wall has no wall bonus.

- **AC6 — Casualties & survivor return (P1/P2).** On resolution the defender's **garrison and every
  reinforcement** are reduced by their losses, and the **attacker's movement troops** by theirs —
  persisted, **exactly once**, surviving a restart. The **attacker's survivors** (if any) are sent
  home as a **return** movement and rejoin the home garrison on arrival; if the attacker is wiped out,
  no return is created.

- **AC7 — Battle report.** Every resolution persists a **battle report** visible to **both** the
  attacker and the defender: the kind (attack/raid), each side's forces and losses, the wall level
  razed, the **luck and morale** factors, and who won. Reports are derived from persisted state + seed
  (P2) and do not depend on who was online.

- **AC8 — Interface.** The player can launch an **attack** or **raid** from the **Rally Point** (target
  coordinate, per-unit counts, and the mode), and read their **battle reports** (an inbox list with a
  detail view showing forces, losses, wall damage, luck, and morale). Unavailable actions are not
  offered (and are rejected server-side regardless).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/view combat (redirected to login). |
| **Player** | Attack/raid **from their own** village; defend automatically (their garrison + reinforcements help); read **their own** battle reports (as attacker or defender). | Send another player's troops; send troops the garrison lacks; forge the target, composition, casualties, luck, or report; read others' reports; attack their own village. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel time (AC1). | — (superset). |
| **System** | *(system-initiated)* Resolve battles at their due time — compute casualties, apply them, send survivors home, emit reports (AC3–AC7); never mid-flight. | — |

## Out of scope

- **Loot / plunder / Cranny protection** (a raid's resource gain) → slice **011**; in 009 survivors
  return **empty** (the fight and casualties only).
- **Catapults damaging buildings** → slice **011** (catapults still fight as units here, but raze no
  buildings); only **rams vs the Wall** are modelled.
- **Scouting** (espionage movements + separate scout combat) → slice **010**; **Scout**-role units do
  not participate in the main battle.
- **Conquest / loyalty / administrators** → slice **014**; an attack never transfers ownership here.
- **Trapper** (Gaul), **Brewery** (Teuton), hero, and **alliance/confederation** defense sharing →
  later slices.
- **In-transit recall** of an attack/raid (faithful — outgoing attacks cannot be recalled, §8.3).

## Decisions

- **Attack & raid are new `troop_movements` kinds** on the 007 engine; the attacker's **survivors
  return** via the existing `return` kind (rejoining the home garrison). Combat resolution runs in the
  **application** layer (it needs economy/unit/combat rules and writes several parties), claimed by a
  dedicated due-event processor — distinct from the reinforcement apply.
- **The battle math is pure domain** (`combat.rs`): `resolve_battle(attacker pools, defender profile,
  wall, populations, luck) -> Outcome` (loss fractions + razed wall), unit-tested in isolation (P3).
  Exact constants — the loss exponent, luck range, morale exponent, base defense, per-tribe wall bonus
  and durability — are **balance data** (P7).
- **Luck is seeded**: `luck(worldSeed, movementId)` via the 006 SplitMix64 hash mapped into
  `[1−L, 1+L]`. Re-resolving is identical (P2/P6).
- **Casualty rounding is deterministic** (round half to even on the per-type loss), applied to the
  defender's garrison first then its reinforcement groups, and to the attacker's movement troops.
- **Defender reinforcements use base (level-0) Smithy strength**, while the target's own garrison is
  scaled by the target's Smithy levels. The home village's *own* upgrades do not follow its troops
  abroad in this slice (a faithful refinement would carry per-group levels); reinforcements still
  contribute their full base def — only the upgrade bonus is omitted.
- **The Wall is a new constructable building** (`BuildingKind::Wall`), tribe-flavoured by its balance
  bonus/durability; **rams** are identified by a `siege` tag in unit balance (`Ram`/`Catapult`).
- The target village id is **fixed at send** (a later ownership change does not redirect an attack in
  flight) — mirrors 007.
