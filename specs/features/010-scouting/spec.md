# Feature 010 — Scouting

**Status:** Draft
**Depends on:** 004 (Scout-role units + Academy research), 005 (garrison + reinforcements), 006 (distance), 007 (movement engine), 009 (combat rails: attack/raid kinds, due-event resolution, battle reports, report inbox)
**Roadmap:** M4 · slice 010 · GDD §6.1, §9.4 — information warfare; reveal a village's hidden resources and defenses, either with a dedicated reconnaissance run or by sending scouts **along with an attack**.

## Goal

A player can **scout** another village two ways: a **standalone scout mission** (scouts only — spy and
come home, no battle), or by **including scouts in an attack/raid** (009) so the village is **scouted
in addition to being attacked**. In both cases, **at the instant of arrival** the server runs the
**espionage step separately, first** (GDD §9.4 step 1, P4/P6): the attacker's scouting strength fights
the defender's **counter-espionage** (stationed scouts), **power-law losses fall only on the attacking
scouts**, and — if any survive to return — an **intel report** reveals what the attacker chose to spy
on (**resources** *or* **defenses**). Detection is **stealthy**: the defender learns scouting occurred
only if their counter-espionage **killed at least one** attacking scout. This delivers GDD §7.3's
promise — troop counts, resources, and defenses are hidden and revealed only by scouting — and lays
the intel rails that siege targeting (011) and conquest (014) read.

## Concepts

- **Two scouting paths.**
  - **Standalone scout mission** — a new movement kind carrying **only Scout-role units**: it runs the
    espionage step and **nothing else** (no main battle, no wall razing, no loot); survivors return home.
  - **Scouts within an attack/raid** — an attack or raid (009) whose composition **includes** scouts.
    At arrival the **espionage step runs first**, *then* the **main battle** (009) proceeds with the
    non-scout units. Faithful to Travian: a single send both scouts the village **and** attacks it.
- **Scouts never fight the main battle.** A Scout-role unit (`UnitRole::Scout`, already excluded from
  attack/defense power in 009) contributes **nothing** to the main battle in either path — it is
  reconnaissance only. Its only combat is the separate espionage step.
- **Scout target:** chosen at send — **Resources** or **Defenses** (faithful; one mission cannot get
  both). *Resources* reveals the target's **current stored resources** (computed-on-read at arrival,
  P1). *Defenses* reveals the **stationed troops** (target garrison + every reinforcement) and the
  **Wall level**.
- **Scouting strength:** a per-unit **balance** attribute (`scouting`), nonzero only for Scout-role
  units, used **both** as espionage power (attacking) and counter-espionage power (defending) — the
  faithful single-value model. Non-scout units have `scouting = 0`.
- **Espionage resolution:** attacker power `Σ(attacking scouts × scouting)` is weighed against the
  defender's counter power `Σ(defending scouts × scouting)` over the garrison and **all**
  reinforcements. Attacking scouts take **power-law losses** in the defender/attacker power ratio
  (exponent is balance data); **defending scouts are never lost** (counter-espionage is free). **No
  morale, no luck, no wall bonus** — espionage is deterministic from the persisted counts (P2/P6).
- **Survival & intel delivery.** Standalone: surviving scouts return; intel is delivered **iff ≥ 1**
  scout survives. Combined: the espionage survivors **then take the attacker's main-battle loss
  fraction** (009 — a lost normal attack wipes the attacker, scouts included); intel is delivered
  **iff ≥ 1** scout is among the army's **returning survivors** — so an annihilated attacker brings
  **no intel** even if espionage succeeded.
- **Reports.** Standalone yields a scouter-facing **intel report** (+ a defender **scout-notification**
  only when detected). Combined yields the 009 **battle report** to both parties (the attacker's also
  carries the intel; the defender's **flags** that scouting occurred — only when detected). All land in
  the existing report inbox; all derive from persisted state, independent of who was online (P2).

## User stories

- As a **player**, I want a standalone scout run to learn an enemy's **resources** before I raid, so I
  can judge the payoff without committing an army.
- As a **player**, I want to scout an enemy's **defenses** so I don't throw troops at a wall I can't
  break.
- As a **player**, I want to **send scouts with my attack** so one movement both spies and strikes.
- As a **player**, I want my surviving scouts to bring the intel home, and to be told plainly when a
  mission (or the army carrying it) was lost.
- As a **defender**, I want my stationed scouts to **counter** enemy espionage automatically, and to be
  warned when an enemy spied successfully enough to lose scouts to me.

## Acceptance criteria

> All espionage is server-authoritative (P4) and deterministic given the persisted inputs (P6/P2): the
> scouts sent, the target, the chosen intel, travel time, the losses, the revealed intel, and the
> reports are computed server-side; the client only issues the command.

- **AC1 — Send a standalone scout mission.** Given the player's village with a garrison containing
  Scout-role units and a target (another existing village on a **different tile**, **not the player's
  own**), when the player sends a chosen count of their **scouts** with a chosen **target type**
  (Resources | Defenses), those scouts **leave the garrison** and a **Scout** movement is created
  arriving at `now + travelTime` (007 formula, paced by the slowest scout × world speed, P7).

- **AC2 — Send scouts within an attack/raid.** Given a garrison with combat units **and** Scout-role
  units, when the player sends an **attack** or **raid** (009) whose composition **includes scouts** and
  picks a **scout target** (Resources | Defenses, **defaulting to Defenses** when none is chosen), the
  scouts leave the garrison **with the army** and travel as **one movement** (paced by the slowest unit,
  P7). The scouts will both **scout** and ride with the attack; they add **no** attack power.

- **AC3 — Send rejected (nothing removed).** Rejected with **no troops removed** when: a requested
  count exceeds the garrison; the composition is empty; the target tile holds no village; or the target
  is the sender's own village. Additionally, a **standalone Scout mission** is rejected if **any**
  requested unit is **not** a Scout-role unit (scouts-only). (Only the owner's own troops can be sent,
  P4.)

- **AC4 — Deterministic espionage (P6/P2).** The espionage outcome — attacker scout losses, whether the
  defender is detected, and the revealed intel — is a **pure function** of the persisted inputs (both
  sides' scout counts and the `scouting` balance, the target's resources/troops/Wall at arrival), using
  **no luck, no morale, no wall bonus, and no wall-clock or online state**. Re-resolving the same inputs
  yields the **same** result.

- **AC5 — Espionage formula & losses.** Attacker power = `Σ(attacking scout count × scouting)`;
  defender counter power = `Σ(defending scout count × scouting)` over the target's **garrison and every
  reinforcement**. Attacking scouts lose a **power-law** fraction of the defender/attacker power ratio
  (exponent is balance data): if defender counter power is **0**, losses are **0**; if it **meets or
  exceeds** attacker power, **all** attacking scouts are lost. **Defending scouts take no losses.**
  Losses are persisted **exactly once**, surviving a restart (P1/P2).

- **AC6 — Resolution order (GDD §9.4).** At arrival the server resolves the **espionage step first**
  (AC5: attacker scouts vs defender counter-espionage → scout losses + intel + detection). A
  **standalone Scout mission stops there** (no main battle). An **attack/raid that includes scouts**
  then proceeds to the **main battle** (009, unchanged) with the non-scout units — espionage never
  alters the main-battle inputs or outcome.

- **AC7 — Combined survival & intel delivery.** In a combined attack, the espionage-surviving scouts
  **then take the attacker's main-battle loss fraction** (009: a lost normal attack loses **all**
  attacker units including scouts; a won attack or a raid applies the proportional fraction). Intel is
  delivered home **iff ≥ 1** scout is among the army's **returning survivors**; an annihilated attacker
  delivers **no intel** even if espionage gathered it. In a **standalone** mission (no main battle),
  intel is delivered **iff ≥ 1** scout survives AC5.

- **AC8 — Stealth detection.** Detection fires **iff ≥ 1** attacking scout died **to counter-espionage**
  (AC5). Standalone: a detected mission persists a **scout-notification** to the defender (village
  scouted + scouts destroyed); an **undetected** mission (zero counter-kills, e.g. no defending scouts)
  yields **no** defender report. Combined: the defender's **battle report** (009) **flags** that
  scouting occurred (and the target type) when detected, and shows nothing extra when undetected.

- **AC9 — Intel content by target type.** When intel is delivered (AC7), the attacker's report reveals,
  for **Resources**, the target's current stored resources of each type (computed-on-read at arrival,
  P1); for **Defenses**, the target's stationed troop counts (garrison + reinforcements, by unit type)
  and the current **Wall level**. When intel is **not** delivered, the report shows the mission as lost
  with **no intel**.

- **AC10 — Survivor return (P1).** Standalone: surviving scouts are sent **home** as a `return`
  movement; if all scouts die, **no return** is created. Combined: surviving scouts return **with** the
  army's 009 return (rejoining the home garrison on arrival). Scouting itself moves **no resources** and
  **changes no troops on the defender's side**.

- **AC11 — Reports.** Standalone persists an **intel report** for the **scouting player** (target,
  target type, scouts sent/lost, revealed intel or "no intel") and, when detected, a **scout-
  notification** for the defender. Combined persists the 009 **battle report** to both parties — the
  attacker's also carrying the intel (or a sibling intel report; plan decision), the defender's flagging
  detected scouting (AC8). All appear in the existing inbox with a detail view; all derive from
  persisted state (P2).

- **AC12 — Interface.** From the **Rally Point** the player can launch a **standalone scout mission**
  (scout counts + target type) and can **add scouts + a target type to an attack/raid** (the scout
  selector appears when scouts are in the composition). Intel reports are readable in the inbox (list +
  detail showing scouts sent/lost and the revealed intel). Non-scout units are not offered for a
  standalone scout mission (and are rejected server-side regardless, AC3).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/view scouting (redirected to login). |
| **Player** | From their **own** village: send a standalone scout mission, or include scouts in an attack/raid; counter enemy scouts automatically (their stationed scouts + reinforcements' scouts defend); read **their own** intel/battle reports and scout-notifications. | Send another player's scouts; send scouts/units the garrison lacks; send **non-scout** units as a standalone scout mission; scout their own village; forge the target, target type, losses, detection, or revealed intel; read others' reports. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel time (AC1/AC2). | — (superset). |
| **System** | *(system-initiated)* Resolve missions at their due time — run espionage, decide detection, gather intel, then (combined) the main battle, return survivors, emit reports (AC4–AC11); never mid-flight. | — |

## Out of scope

- **Wall / Palace / Residence boosting counter-espionage.** In this slice counter-espionage is
  **scouts only** (no building bonus); a faithful refinement is deferred (Decision).
- **Building-level intel for catapult targeting** (revealing exact building levels to aim catapults) →
  slice **011**; *Defenses* reveals troops + Wall **level** only, not a full building list.
- **Loot / Cranny / resource transfer** — scouting never moves resources; a *combined raid's* loot is
  still governed by 009/011, unchanged by the scouts riding along.
- **In-transit recall** of a scout or attack mission (faithful — outgoing attacks/scouts cannot be
  recalled, §8.3).
- **Per-reinforcement Smithy levels / scouting upgrades** — scouting uses base `scouting` strength
  (mirrors 009's reinforcement-strength simplification).

## Decisions

- **`scouting` is a new balance attribute** on units (`specs/balance/units.toml`), default `0`, nonzero
  only for Scout-role units; it serves as **both** espionage and counter-espionage power (faithful
  single-value model). Starting values (subject to tuning, P7): Roman *Equites Legati* and Gaul
  *Pathfinder* higher, Teuton *Scout* lower — consistent with their existing speed/cost tiers. Exact
  numbers live in balance, not here.
- **A new `Scout` movement kind** for standalone missions (alongside Attack/Raid/Reinforce/Return);
  the migration extends the `troop_movements` kind constraint. Standalone resolution runs in the
  **application** layer via `process_due_scouts`.
- **Combined attacks extend 009's `process_due_combat`:** when an attack/raid's composition includes
  scouts, the processor runs the **espionage step first** (GDD §9.4 step 1) — applying scout losses,
  gathering intel, deciding detection — **then** the existing main battle on the non-scout units. The
  espionage-surviving scouts are carried into the attacker's main-battle casualty application and the
  return. `order_attack` (009) is extended to accept scouts in the composition plus an optional scout
  **target type** (defaulting to **Defenses** when scouts are present and none is chosen).
- **The espionage math is pure domain** (`scouting.rs`): `resolve_scouting(attacker_power,
  defender_power, rules) -> ScoutOutcome` (attacker loss fraction + detected flag), unit-tested in
  isolation (P3). The **loss exponent** and any clamp are balance data (P7). Intel gathering (reading
  the target's resources/troops/Wall) is application-layer (it touches persisted state), not domain.
- **Report shape** — whether intel extends the `battle_reports` table or gets a sibling `scout_reports`
  table, and whether a combined attack's intel folds into the battle report or rides as a separate
  report, is a **plan** decision; the spec only requires both reach the shared inbox.
- **Intel is a snapshot at arrival**, computed-on-read (P1): resources via the 002 accrual model,
  troops/Wall from persisted state at the resolution instant — never a live feed.
- **Determinism without RNG:** espionage has **no luck/morale**, so its outcome is fully determined by
  the persisted counts — no seed needed (still reproducible/explainable, P6/P2). The combined main
  battle keeps 009's seeded luck, unchanged.
- The target village id is **fixed at send** (a later ownership change does not redirect a mission in
  flight) — mirrors 007/009.

## Open questions

- **Counter-espionage boost (Wall/Palace).** Faithful Travian raises detection with certain buildings.
  Proposed: **defer** (scouts-only counter in 010). Flagged in *Out of scope*.
- **Intel granularity for *Defenses*.** Exact counts + Wall level (proposed, faithful T4) vs. a
  rounded/banded estimate. Proposed: **exact counts**; banding is a balance refinement, not a slice
  gate.
- **Combined intel report shape.** Fold intel into the attacker's battle report vs. emit a separate
  intel report alongside it. Proposed: **plan-level** call; spec is agnostic.
