# Feature 004 — Tribes & units

**Status:** Verified
**Depends on:** 003 (construction & build queue — new buildings, spending, due-events)
**Roadmap:** M2 · slice 004 · GDD §5, §6 — tribe identity and the unit attribute model.

## Goal

Players **choose a tribe** (Romans, Teutons, Gauls) at registration and gain access to that tribe's
**unit roster**: they research unit types in the **Academy** and improve them in the **Smithy**.
Romans get their trait — a **parallel build queue** (one resource field *and* one center building at
once). This slice establishes tribe identity and the full unit attribute model; **training the units
is slice 005**.

## Concepts

- **Tribe:** chosen **once at registration**, immutable thereafter (GDD §5). It determines the unit
  roster and tribe traits. The tribe is account-level; every village a player founds carries it.
- **Unit type:** a per-tribe definition (~10 per tribe) carrying the GDD §6.2 attributes —
  `attack`, `defenseInfantry`, `defenseCavalry`, `speed`, `carryCapacity`, `cropUpkeep`, `cost`,
  `trainTime` — plus a **role** (infantry / cavalry / scout / siege / expansion), the building it
  will train in (005), **research requirements** (building levels), and **research cost & time**.
  All values are **balance data**, not code.
- **Research (Academy):** a unit type must be researched **per village** before it can be trained
  there (005). Research costs resources and takes time; completion is a **due-event** (P1). Each
  tribe's **tier-1 unit** (Legionnaire / Clubswinger / Phalanx) needs **no research** — it is
  trainable from the start.
- **Smithy upgrade:** raises a researched unit type's combat strength **in levels, per village**.
  Each level costs resources and takes time (due-event). The stored level feeds the combat formula
  in slice 009; in this slice the level itself is the observable outcome.
- **Roman trait — parallel queue:** a Roman village may have **one active field order and one active
  center-building order simultaneously**; other tribes keep the single active order from 003.

## User stories

- As a **visitor**, I want to pick a tribe when I register, so that my play style is set from the start.
- As a **player**, I want to research my tribe's units in the Academy, so that I can train them (005).
- As a **player**, I want to upgrade my units in the Smithy, so that they fight better (009).
- As a **Roman player**, I want to build a field and a center building at the same time, so that my
  trait is worth something.

## Acceptance criteria

> All actions are server-authoritative (P4): tribe, costs, times, requirements, and completions are
> computed and enforced server-side; the client only issues commands.

- **AC1 — Tribe chosen at registration.** Given the registration form, when a visitor registers with
  tribe ∈ {Romans, Teutons, Gauls}, then the account stores that tribe and the starting village
  carries it. A registration with a missing or unknown tribe is **rejected server-side**.

- **AC2 — Tribe is immutable.** No interface exists to change the tribe after registration; any
  attempt is rejected.

- **AC3 — Pre-004 accounts are backfilled.** Accounts (and their villages) created before this slice
  have tribe **Gauls** after migration (the recommended beginner tribe; no 004-relevant trait, so no
  retroactive advantage). Migration-boundary test required.

- **AC4 — Complete per-tribe rosters.** Balance data defines, for **each** of the three tribes, a
  roster of **10 unit types**, each with all §6.2 attributes (attack, defense vs. infantry, defense
  vs. cavalry, speed, carry capacity, crop upkeep, cost, train time), a role, research requirements,
  and research cost/time. Loading **fails fast** if any tribe's roster is incomplete or any attribute
  is missing. The domain exposes a tribe's roster.

- **AC5 — New buildings: Barracks, Academy, Smithy.** These become constructable through the 003
  build catalog/mechanics, with prerequisites (balance): Barracks ← Main Building ≥ 3;
  Academy ← Main Building ≥ 3 and Barracks ≥ 3; Smithy ← Main Building ≥ 3 and Academy ≥ 1.
  Ordering one with unmet prerequisites is rejected (003 AC4 applies unchanged).

- **AC6 — Start a research.** Given a village with an Academy, a unit type of the owner's tribe that
  is unresearched, whose building requirements are met, with **no research in progress** in that
  village and sufficient resources, when the player orders the research, then resources are settled
  and the **research cost debited**, and a research order is created completing at
  `now + researchTime ÷ worldSpeed`.

- **AC7 — Research rejected.** A research order is rejected — with **nothing debited and no order
  created** — when any of: the unit is already researched; a research is already in progress in the
  village; the unit's building requirements are unmet (including: no Academy); resources are
  insufficient; the unit does not belong to the owner's tribe.

- **AC8 — Research completes exactly once (P1/P2).** When the order's due time passes, the unit
  becomes **researched** in that village exactly once; the state is persisted, survives a restart,
  and a pending order still completes after one.

- **AC9 — Tier-1 needs no research.** Each tribe's first unit (Legionnaire, Clubswinger, Phalanx) is
  researched-by-default in every village of that tribe — without any stored research row or order.

- **AC10 — Start a Smithy upgrade.** Given a village with a Smithy, a **researched** unit type whose
  current upgrade level `L` satisfies `L < smithyLevel` and `L < 20` (max, balance), no upgrade in
  progress in that village, and sufficient resources, when the player orders the upgrade, then the
  cost (balance: per unit and level) is debited after settling and an upgrade order is created
  completing at `now + upgradeTime ÷ worldSpeed`.

- **AC11 — Smithy upgrade rejected.** Rejected with nothing debited when any of: unit unresearched
  (tier-1 counts as researched); an upgrade already in progress; `L ≥ smithyLevel` or `L ≥ 20`;
  insufficient resources; no Smithy; wrong tribe.

- **AC12 — Upgrade completes exactly once (P1/P2).** At due time the unit type's level becomes
  `L + 1` in that village exactly once; persisted; survives restart.

- **AC13 — Roman parallel build queue.** Given a **Roman** village with an active **field** order,
  ordering a **center-building** upgrade (or vice versa) succeeds — both run concurrently; a second
  order of the **same** category is rejected. Given a **non-Roman** village with any active order,
  **any** second order is rejected (003 AC3 unchanged). Enforced server-side even under concurrent
  requests.

- **AC14 — Speed scales research & upgrades (P7).** Research and Smithy upgrade durations scale
  inversely with the configured world speed; no wall-clock duration is hardcoded.

- **AC15 — Interface.** Registration offers the three tribes with a short description each (choice
  required). The village page shows the player's tribe. With an Academy built, the player can view
  their tribe's units with research state, requirements, cost and time, order a research, and see a
  **live countdown** while one runs. With a Smithy built, the same for upgrade levels per researched
  unit. Unaffordable or unavailable actions are not offered (and are rejected server-side regardless).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | Choose a tribe as part of registration (AC1). | Research/upgrade/view any village (redirected to login). |
| **Player** | Research and upgrade units in **their own** villages; see their own tribe's roster and queue state. | Act on another player's village; change tribe (AC2); research another tribe's units; forge cost/time/level/requirements; exceed the queue rules (AC6/AC10/AC13). |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed configuration scales research/upgrade times (AC14). | — (superset). |
| **System** | *(system-initiated)* Apply research and upgrade completions at their due time (AC8/AC12); backfill tribe at migration (AC3). | — |

## Out of scope

- **Training units** (Barracks/Stable/Workshop queues, garrison, crop upkeep effects) → slice 005.
  Units exist here as researchable/upgradable definitions only.
- **Stable, Workshop, Residence/Palace buildings** → 005/013. Units whose research requires them
  (cavalry, siege) remain unresearchable until those slices — the requirement data ships now.
- **Combat effects of Smithy levels** (the % formula) → slice 009; the level is stored now.
- **Teuton trait** (Cranny plunder) → with Cranny/loot (011). **Gaul trait** (fastest cavalry) is
  already expressed in the unit speed data. **Special buildings** (Brewery, Trapper) → later.
- **Per-tribe walls** → combat (009). **Per-tribe merchants** → trade (008).
- **Scout/settler/administrator functionality** → 010/013/014; they are roster entries only.
- **Building-level speedups** for research/upgrade times (higher Academy/Smithy working faster) →
  not in baseline; times depend on balance base values and world speed only.

## Decisions

- **Tribe is account-level**, stored on the user and stamped onto each village at founding; the
  existing nullable village tribe column becomes the per-village copy. Backfill default: **Gauls**.
- **Tier-1 auto-research is a domain rule** (a unit with no research requirements and zero research
  cost/time is implicitly researched), not seeded data — no backfill rows needed (AC9).
- **One research and one Smithy upgrade** may run concurrently in a village (they are different
  queues), each capped at one active order; both independent of the construction queue.
- **Roman parallel queue lanes are 'field' and 'building'**; enforcement must hold under races
  (DB-level guarantee, as in 003).
- **Roster contents are faithful Travian-style** (3 × 10 units incl. scouts, siege, administrator,
  settlers); all numbers live in `specs/balance/units.toml` and are tunable without spec changes.
