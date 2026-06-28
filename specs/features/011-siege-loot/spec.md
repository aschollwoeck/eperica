# Feature 011 — Siege & loot

**Status:** Verified
**Depends on:** 003 (construction — the Cranny is a built building), 009 (combat resolution + battle reports), 002 (resource accrual + warehouse/granary caps), 007 (the `return` movement that carries loot home)
**Roadmap:** M4 · slice 011 · GDD §9.3–9.4, §8.4 — the **destruction + plunder** loop that turns a won fight into damage and stolen resources.

## Goal

A won attack or raid now **takes something home**. After the 009 main battle, surviving **catapults**
**raze levels of a targeted building** (the attacker aims; a seeded-random building is hit otherwise),
and surviving attackers **loot resources** — bounded by their **carry capacity**, shielded by the
defender's **Cranny** (which **Teutons partially bypass**). The loot **rides the survivor return** and
is **credited at home on arrival** (capped at the attacker's storage); the **battle report** shows the
resources stolen and the building damage. This completes the faithful raiding loop (GDD §9.4 steps 4 &
6) and adds the **Cranny** building that defends against it.

## Concepts

- **Catapults damage buildings.** Catapults already fight in the 009 main battle (in the infantry
  pool) and take casualties there. **If the attacker prevails** and **≥ 1 catapult survives**, their
  combined power **razes whole levels** of one **targeted building** — `floor(catapultPower /
  durability)` levels, capped at the building's current level (mirrors how **rams** raze the **Wall**,
  009). The attacker **picks the target building** at send; if none is chosen (or the chosen building
  is absent/level 0), a **seeded-random existing building** is hit. Rams→Wall is unchanged (009).
- **Loot.** On a fight the attacker **survives** (a won attack, or a raid with survivors), the
  surviving troops **plunder**: the **lootable** amount of each resource is `max(0, settledStored −
  crannyProtection)`; the **total** taken is bounded by the surviving troops' summed **carry capacity**
  and distributed **proportionally** across the four resource types. The loot is **debited from the
  target** at resolution and **carried home** on the return.
- **Cranny.** A new constructable building (003) that **hides a quantity of each resource** from
  looting — a per-level capacity (balance). Resources at or below the protected amount **cannot be
  taken**. A village with no Cranny protects nothing (only whatever isn't there is safe).
- **Teuton bypass.** A **Teuton** attacker ignores a configured **fraction** of the Cranny's
  protection (GDD §5.2), looting more than other tribes against the same Cranny.
- **Carried home & credited.** Looted resources travel on the existing **`return`** movement (007) and
  are **credited at the attacker's village on arrival** — settled forward then **capped at warehouse/
  granary** (002; overflow is lost, like trade delivery 008). Conservation: the target loses the loot
  **once** at resolution; the attacker gains it **once** at return arrival (minus any storage overflow).
- **Report.** The battle report gains **resources looted** and **building damage** (kind, level
  before → after), shown to both parties (GDD §9.5), derived from persisted state + seed (P2/P6).

## User stories

- As a **player**, I want my **raids to bring resources home**, so raiding funds my growth.
- As a **player**, I want to **aim my catapults** at an enemy building (their Warehouse, Smithy, …),
  so I can cripple a rival.
- As a **player**, I want to build a **Cranny**, so a chunk of my resources survives a raid.
- As a **Teuton**, I want my raiders to **dig past** an enemy's Cranny, true to my tribe.
- As a **defender**, I want the report to show **exactly what was stolen and damaged**.

## Acceptance criteria

> All siege and loot is server-authoritative (P4) and deterministic given the persisted inputs + the
> world seed (P6/P2): the targeted building, the random fallback, the loot amounts and distribution,
> the building damage, and the report are computed server-side; the client only chooses the order.

- **AC1 — Choose a catapult target.** When the player sends an **attack/raid** whose composition
  includes **catapults**, they may pick a **target building**; it is persisted on the movement. With no
  choice, the target is left unset (a seeded-random building is selected at resolution, AC2). With no
  catapults the attack carries no catapult target.

- **AC2 — Catapult building damage.** At resolution, **iff the attacker prevails and ≥ 1 catapult
  survives** the main battle, the catapults raze `floor(catapultPower / durability)` whole levels
  (capped at the building's level) of the **target building** — the chosen one if it exists at level
  ≥ 1, else a **seeded-random** building the target currently has (the Wall is excluded — it is rams'
  job). If the attacker loses, or no catapult survives, or the target has no eligible building, **no
  building is damaged**. The razed levels and the building are recorded for the report and applied to
  the target **exactly once**.

- **AC3 — Loot bounded by carry capacity.** When the attacker has **surviving troops** after the
  battle (a won attack or a raid with survivors), they loot: per resource, `lootable = max(0,
  settledStored − crannyProtection)`; the **total** loot `= min(Σ lootable, Σ survivingCarryCapacity)`,
  split across the four resources **in proportion** to each one's lootable share. The loot is
  **subtracted from the target's stored resources** at the resolution instant (settled-on-read, P1),
  **exactly once**.

- **AC4 — Cranny protection.** The target's **Cranny** shields `crannyProtection` of **each** resource
  type (a per-level capacity, balance); only the surplus above it is lootable. A village **without** a
  Cranny shields nothing. Protection is per resource type (symmetric across the four). *(110: multiple
  Crannies **sum** their protection; a single one equals this value.)*

- **AC5 — Teuton bypass.** A **Teuton** attacker reduces the effective Cranny protection by a
  configured **bypass fraction** (balance), so against the same Cranny a Teuton loots strictly more
  than a non-Teuton (until everything is taken). Non-Teuton attackers face the full protection.

- **AC6 — Loot carried home & credited (P1/P2).** The looted resources **ride the survivor `return`**
  movement (007). On the return's arrival they are **credited to the attacker's village**, settled
  forward to that instant and **capped at warehouse/granary** (002; overflow lost, like 008). The
  credit happens **exactly once**, surviving a restart. (Conservation: debited once from the target at
  resolution, credited once to the attacker at arrival, minus storage overflow.)

- **AC7 — No survivors, no loot.** If the attacker is **wiped out**, **nothing** is looted, no building
  is damaged (no catapult survives), and **no return** is created (009 unchanged).

- **AC8 — Deterministic (P6/P2).** The random-fallback building choice draws from the **world seed +
  movement id** (006/009 hashing), and all loot/damage math is a pure function of the persisted inputs.
  Re-resolving the same battle yields the **same** loot, distribution, target, and razed levels.

- **AC9 — Report.** Every resolution's **battle report** (both parties, P4) shows the **resources
  looted** (per type) and the **building damage** (kind + level before → after, or "none"), alongside
  the 009 forces/losses/wall/luck/morale. Reports are derived from persisted state + seed (P2).

- **AC10 — Build a Cranny.** A player can **build and upgrade** a **Cranny** like any center building
  (003: costs, prerequisites, build time as due-events, demolition); its level sets the protection
  (AC4). Roles/permissions follow 003 (only the owner builds in their own village, P4).

- **AC11 — Interface.** The **Rally Point** send form gains a **catapult target** building selector
  (shown when catapults are in the composition). **Battle reports** show loot + building damage. The
  **Cranny** appears among buildable buildings. Unavailable actions aren't offered (and are rejected
  server-side regardless, P4).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/build/view (redirected to login). |
| **Player** | Aim catapults & loot **from their own** attacks; build/upgrade a **Cranny** in their own village; have their Cranny protect automatically; read **their own** reports (loot + damage). | Forge the target, loot amounts, damage, or report; loot more than carry capacity / past a non-bypassed Cranny; build in another's village; read others' reports. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel/build times. | — (superset). |
| **System** | *(system-initiated)* Compute loot + building damage at resolution, debit the target, carry loot on the return, credit it (capped) on arrival, emit the report — exactly once (AC2–AC9). | — |

## Out of scope

- **Conquest / loyalty / administrators** (resolution step 5) → slice **014**.
- **Trapper** (Gaul — traps attackers) and **Brewery** (Teuton) → later tribe-trait slices; this slice
  adds **only** the Teuton Cranny bypass and the Cranny building.
- **Multiple catapult targets** (T4 allows two at a high Rally Point) — **one** target building in 011.
- **Catapults vs the Wall** — catapults hit **buildings**; the **Wall** is rams' job (009), unchanged.
- **Oasis / wild-animal loot** (§7.4) → slice **012**.
- **Per-building catapult durability differences** — a single durability constant in 011 (a
  per-building table is a later balance refinement).

## Decisions

- **Catapult building damage is pure domain** (`combat.rs`): catapult power is summed from the
  **surviving** catapults (Smithy-scaled), and `razed = floor(power / catapult_durability)` capped at
  the target building level — symmetric with the existing **ram→Wall** razing. The durability is
  balance (`combat.toml`). Catapults remain in the **infantry** combat pool (009) — they fight and die
  before they raze.
- **Target selection** is application-layer (it reads the target's buildings): the chosen building if
  present (level ≥ 1), else a **seeded-random** pick over the target's existing non-Wall buildings via
  the 006/009 hash (world seed + movement id). The catapult target rides the movement row (a new
  nullable `catapult_target` building-kind column), mirroring 010's `scout_target`.
- **Loot math is pure domain**: given each resource's `settledStored`, the per-type Cranny protection
  (already Teuton-adjusted), and the survivors' total carry capacity → the `(wood,clay,iron,crop)` loot
  bundle (proportional, capacity-bounded). The application settles the target's resources to the
  resolution instant (002), computes protection from the Cranny level (balance, Teuton-adjusted), runs
  the domain split, and **debits the target** in the 009 `apply_battle` transaction.
- **The return carries the loot.** `BattleApply` gains the loot bundle + building damage; `apply_battle`
  attaches the loot to the survivor `return` movement (a new nullable resource bundle on
  `troop_movements`) and records report fields — all in its existing single transaction. The **007
  `return` apply** is extended to **credit** any attached loot (settle + `deposit_capped`, mirroring the
  008 trade deliver), exactly once.
- **Cranny is a new `BuildingKind::Cranny`**, threaded through the balance/repo/web mappings like the
  009 **Wall**, with a per-level protection capacity in balance and a population entry. The **Teuton
  bypass fraction** is balance (`combat.toml`).
- **Battle report** gains `loot` (resource bundle) and `building_damage` (kind + before/after) columns;
  `BattleReportView` + the report templates surface them (GDD §9.5/§9.6).
- **Determinism**: the same seeded hash that drives 009 luck drives the random building pick, so a
  battle's loot, target, and damage are fixed at send and reproducible (P2/P6).

## Open questions

- **Loot rounding / distribution.** Proportional split with deterministic rounding (round-half-to-even,
  reusing 009's casualty rounding approach), remainder assigned to the largest share. Proposed:
  **yes** — a single deterministic rule; exact tie-breaking is a Decision, not a slice gate.
- **Cranny protection symmetry.** One protection capacity applied **per resource type** (proposed,
  faithful T4) vs a single shared pool. Proposed: **per-type**, balance-tunable.
- **Random-target eligibility (decided).** The fallback pick **excludes the Wall** (rams' domain) and
  the **Rally Point** (always present, un-razable in T4); it draws from the target's other existing
  buildings (level ≥ 1). The *explicitly chosen* target is likewise rejected if it is the Wall or Rally
  Point (server-enforced, P4).
