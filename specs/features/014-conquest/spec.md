# Feature 014 — Conquest

**Status:** Verified
**Depends on:** 013 (the capital flag + the unconquerable rule, the per-player culture/expansion-slot gate, the Residence/Palace, the `Expansion` unit role), 009 (the troop-movement battle engine + battle reports the conquest step rides on), 005 (training the administrator unit), 002/003 (the per-village economy/buildings that transfer), 006 (the seeded map the transferred village sits on)
**Roadmap:** M5 · slice 014 · GDD §3.3, §3.4, §6.1, §9.4 (step 5), §11.1 — the **aggressive** expansion path: reduce a target village's **loyalty** with **administrators**; at zero, **ownership transfers**. The **capital** cannot be conquered.

## Goal

A player grows not only by **settling** (013) but by **conquering** an enemy village. Every village
carries a **loyalty** value that **regenerates over time**; an attacker who **wins the battle** (009)
and brings a surviving **administrator** (Senator / Chief / Chieftain — the tribe's `Expansion`-role
conqueror) **lowers** that loyalty. When loyalty reaches **zero** and the attacker has a **free
expansion slot** (the 013 CP/Residence gate), the village **changes owner**: it joins the attacker's
empire with its fields, buildings, and stored resources intact, and runs under the new owner from then
on. A player's **capital** (013, a Palace village) **cannot be conquered** — its loyalty never drops.
This slice delivers the loyalty mechanic, the conquest step on the 009 resolution path, and the
ownership transfer; it does **not** add alliances/diplomacy (015) or any new combat math.

## Concepts

- **Loyalty.** A per-**village** value in `[0, 100]` (100 = fully loyal). It is **lazy** (P1): stored
  as `value + lastUpdated`, **regenerating** toward 100 at a balance rate (`loyaltyRegenPerHour`),
  computed on read — there is no global tick. A freshly **founded** (013) or **conquered** village
  starts at a balance value (`startingLoyalty` = 100 for a fresh village; `postConquestLoyalty` for a
  just-taken one). Loyalty is reduced **only** by a successful administrator strike; it is never an
  account-wide pool (unlike CP) — each village has its own.
- **Administrators.** The tribe's `Expansion`-role **conqueror** unit (Roman **Senator**, Teuton
  **Chief**, Gaul **Chieftain** — already in the roster, trained in the **Residence/Palace**, gated
  behind a high Academy/Rally-Point research). Unlike settlers, administrators **fight** (they carry
  attack/defence and take part in the main battle). An administrator that **survives** a **won** battle
  reduces the target's loyalty by a **seeded** amount in `[loyaltyDropMin, loyaltyDropMax]` per
  administrator (the same seeded-RNG discipline as luck, P6). Multiple surviving administrators stack.
- **Conquest (the 009 resolution, step 5).** Conquest is **not** a new movement — it rides an ordinary
  **attack** (009) whose composition includes administrators. After the main battle (and catapults),
  **if** the attacker **won**, **at least one administrator survived**, the target is **not a capital**,
  and the attacker has a **free expansion slot** (013), the surviving administrators reduce the target's
  (regenerated-to-now) loyalty. If the result is **≤ 0**, the village is **conquered**: ownership
  transfers to the attacker in the **same transaction** as the battle apply (exactly-once, P2). If the
  result stays **> 0**, the village keeps its (lower) loyalty and its current owner — only the loyalty
  changed. The battle report records the loyalty before/after and the transfer.
- **Ownership transfer.** A conquered village keeps its **tile, fields, buildings (levels), and stored
  resources**, and becomes owned by the attacker. Its **garrison** is empty (the defenders lost the
  battle that enabled the conquest); any **stationed reinforcements** from third parties are **sent
  home** (a 007 return). The attacker's surviving troops (including the administrators) **return home**
  as usual — they do **not** garrison the new village. Loyalty resets to `postConquestLoyalty`. The
  village's CP now counts toward the **new** owner's village count + culture rate, and **off** the old
  owner's — both players' culture accumulators are **re-anchored** in the same transaction (013 AC1).
  The old owner's **in-flight build/train/movement orders** for that village are cancelled.
- **The capital is unconquerable.** A **capital** village (013, a Palace village; `is_capital`) has its
  loyalty **pinned** — an administrator strike against it reduces **nothing** and never transfers
  ownership. (A capital can still be **attacked, looted, and have buildings razed** — 009/011 — only
  its loyalty/ownership is protected.)
- **The slot gate (shared with 013).** Conquering is an **expansion** like settling: it needs a **free
  expansion slot** — `villageCount < allowedVillages(cp, residences)` for the **attacker** at the
  resolution instant (P4). With no free slot the loyalty is **still reduced** (the strike landed) but
  ownership **does not transfer**, even at loyalty 0 — the attacker cannot hold another village yet.

## User stories

- As a **player**, I want to **train administrators** and send them with an attack to **conquer** an
  enemy village.
- As a **player**, I want each of my villages to **regenerate loyalty** over time, so a partial
  conquest decays if the attacker can't finish it.
- As a **player**, I want my **capital** to be **safe from conquest**, so I have an unbreakable base.
- As a **player**, I want a **battle report** that shows the **loyalty change** and whether the village
  changed hands.
- As a **player**, I want a conquered village to run as **mine** immediately — its economy, queues, and
  defence are now under my control.

## Acceptance criteria

> All conquest is server-authoritative (P4) and reproducible from persisted state + the world seed
> (P2/P6): loyalty, the loyalty drop, the capital exception, the slot gate, and the ownership transfer
> are computed server-side at the resolution instant; the client only issues the attack.

- **AC1 — Loyalty accrues lazily (P1).** Every village has a **loyalty** in `[0, 100]` computed on read
  from stored `(value, lastUpdated)`, **regenerating** toward 100 at `loyaltyRegenPerHour` (balance,
  scaled by world speed, P7) and clamped at 100 — never polled. A fresh/founded village starts at
  `startingLoyalty`.

- **AC2 — Administrators are trainable conquerors.** Each tribe's administrator (Senator/Chief/
  Chieftain, `Expansion` role) is **researched** (Academy/Rally-Point requirements) and **trained** in
  the **Residence/Palace** (005 path, enabled in 013). They carry attack/defence and **participate in
  the main battle** like any unit (they are **not** excluded like settlers/scouts).

- **AC3 — An administrator strike lowers loyalty (P6).** When an **attack** (009) that includes
  administrators **wins** the main battle and **≥ 1 administrator survives**, the target's loyalty
  (regenerated to the resolution instant) is reduced by `Σ drop(adminᵢ)` where each `drop` is a
  **seeded** value in `[loyaltyDropMin, loyaltyDropMax]` (the 009 luck RNG discipline, P6). A **lost**
  battle, or **no surviving administrator**, changes loyalty by **nothing**.

- **AC4 — Conquest at zero loyalty (P4).** If the reduced loyalty is **≤ 0**, the target is **not a
  capital**, and the **attacker has a free expansion slot**, the village is **conquered** — ownership
  transfers to the attacker. If loyalty stays **> 0**, ownership is unchanged (only loyalty dropped).

- **AC5 — The capital cannot be conquered (AC, GDD §3.4).** An administrator strike against a **capital**
  reduces its loyalty by **nothing** and **never** transfers ownership, regardless of the attacker's
  force or slots. (The rest of the battle — losses, loot, razing — resolves normally.)

- **AC6 — The slot gate applies to conquest (shared with 013).** Conquest transfers ownership **only**
  when the attacker has `villageCount < allowedVillages` at the resolution instant. With **no free
  slot**, the loyalty reduction still applies but **no transfer** occurs (even at loyalty ≤ 0).

- **AC7 — Ownership transfer is complete and exactly-once (P2).** On conquest, in **one transaction**:
  the village's `owner` becomes the attacker; its **fields, buildings, and stored resources** are kept;
  its **garrison is emptied** and any **third-party reinforcements** are **returned home** (007); loyalty
  is set to `postConquestLoyalty`; the **new** owner's and the **old** owner's culture accumulators are
  **re-anchored** (the village count/rate moved between them, 013 AC1); and the old owner's **pending
  build/train orders and outgoing movements** for that village are **cancelled**. The attacker's
  surviving troops **return home** (they do not occupy the new village). Orphan-requeue safe.

- **AC8 — A conquered village is the new owner's (AC8 of 013, mirrored).** After transfer the village is
  addressable by the new owner exactly like any of theirs — its economy/queues/garrison/defence and the
  013 **village switcher** include it; the old owner no longer sees it. The village's **tile is
  unchanged** (P6); its **is_capital** is **false** (a conquered village is never a capital — only a
  Palace sets that, 013).

- **AC9 — Loyalty regenerates (decay of a partial conquest).** A village whose loyalty was reduced but
  not taken **regrows** loyalty toward 100 over time at `loyaltyRegenPerHour` (computed on read), so an
  attacker who cannot finish a conquest loses ground — the contest renews.

- **AC10 — The report shows the loyalty change (009 AC8 extended).** The battle report for an attack
  carrying administrators records **loyalty before → after** and whether the village **changed hands**,
  visible to both parties (the 009 report rails), derived from persisted state + seed (P2/P6).

- **AC11 — Conquest interface.** A player can **send administrators** with an attack from the Rally
  Point (the 009 attack form already sends a composition; administrators ride along) and **see** a
  village's loyalty where appropriate (their own villages' loyalty on the village page; a scouted/owned
  view per roles). The report surfaces the loyalty change and the ownership transfer. A conquered
  village appears in the conqueror's **switcher** (013) immediately.

- **AC12 — Determinism & exactly-once (P2/P6).** Loyalty, the seeded drop, the capital exception, the
  slot gate, and the transfer are reproducible from persisted rows + the world seed; the transfer (or
  the loyalty-only change) applies in **one transaction** with the 009 battle apply, exactly-once and
  orphan-requeue safe; the same history yields the same world.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Attack / conquer / view loyalty (redirected to login). |
| **Player** | Train administrators in **their own** Residence/Palace villages; **send** them with an attack against **another** player's village; **conquer** a non-capital enemy village when they win, an administrator survives, and they hold a **free slot**; read **their own** villages' loyalty. | Conquer a **capital**; conquer past their **allowed slot** count; conquer **their own** village; reduce loyalty without winning the battle / without a surviving administrator; transfer ownership client-side; read another player's exact loyalty except via game mechanics (a scout/report). |
| **Moderator** | N/A (considered). | — |
| **Administrator (system role)** | World speed scales the loyalty **regen** + training/travel times; map size bounds reachable targets. | — (superset). |
| **System** | *(system-initiated)* Regenerate loyalty on read; resolve a due attack's **conquest step** — apply the seeded loyalty drop, the capital exception, the slot gate, and the ownership transfer (or loyalty-only change) — exactly once at the resolution instant, from persisted state + seed; re-anchor both players' culture. | — |

## Out of scope

- **Alliances & diplomacy** (Embassy, confederation/war, shared visibility) → slice **015**. Conquest
  here is purely between two players; allied reinforcement of a defender is the existing 007 mechanic
  (third-party stationed troops fight on defence; on conquest they are returned home).
- **Account loss / re-spawn.** A player **can** be reduced to **zero villages** by conquest (their last
  non-capital village taken). Account recovery / re-settling a wiped player is **not** modelled here —
  the player simply owns no villages (the capital rule means a player who built a Palace always keeps at
  least that one).
- **Re-captured-village penalties, loyalty buildings/celebrations, vacation/beginner protection** →
  later. 014 produces loyalty regen + the administrator drop only.
- **New combat math.** Conquest reuses the 009 resolution verbatim (administrators are ordinary
  combatants); only the **post-battle loyalty step** (resolution order step 5) is new.

## Decisions

- **Loyalty is a per-village lazy value mirroring the 013 capital/oasis read paths.** A
  `village_loyalty(village_id, value, updated_at)` row (or a `villages.loyalty/loyalty_updated_at`
  pair), seeded at founding/conquest, **regenerated on read** toward 100 and re-anchored when an
  administrator strike changes it. Like 012's `oasis_bonus`/013's `is_capital`, the **current** loyalty
  is filled on the village read so the conquest resolver and the web see it without an extra query.
- **Conquest rides 009, as resolution step 5.** The attack movement already carries the composition;
  `process_due_combat`/`apply_battle` gain a **loyalty step** after the main battle + catapults: compute
  the surviving-administrator drop (seeded from the battle id, like luck), apply the capital exception +
  the 013 slot gate for the attacker, and either **transfer ownership** or **write the reduced loyalty**
  — in the **same transaction** as the battle apply (P2). No new movement kind, no new scheduler tick.
- **The transfer is a single guarded statement set.** Re-point `villages.owner_id`, clear the garrison,
  return third-party reinforcements (007), reset loyalty, cancel the old owner's pending orders for the
  village, and re-anchor **both** players' `player_culture` (013) — all in the battle apply's tx,
  **guarded** on the village still being owned by the defender (a concurrent change ⇒ the conquest is
  abandoned for that resolution, like the 012 occupy race).
- **Disposition of every `village_id`-keyed dependency (the AC7 principle).** *Assets located in or
  owned by the village pass with it; troops/shipments in transit that can no longer reach a loyal
  village are forfeited.* Concretely: the **garrison** is emptied; **third-party reinforcements**
  stationed in the village are sent home (007); **build/unit/training queues** are cancelled; the
  loser's **outgoing movements** from the village are cancelled, and any troops **returning to** the
  village are **forfeited** (it is no longer theirs — there is no loyal home to arrive at, and leaving
  them would land the loser's army in enemy hands); the village's **own troops stationed elsewhere**
  (reinforcements keyed `home_village = target`), its **in-flight trades**, and its **occupied oases**
  (012) **follow the village to the new owner** (ownership of each is derived from the village, so no
  row change is needed); the pending **starvation check** is left to self-resolve (the emptied garrison
  makes it fire as a no-op); **both players' culture** is re-anchored.
- **Administrators are existing roster units, enabled like settlers were in 013.** Senator/Chief/
  Chieftain already exist (`role = expansion`, `trained_in = residence`); 013 enabled the Residence
  training gate. 014 makes them **conquer** (they are **not** excluded from combat — verify
  `attack_power`/`add_defense` include the `Expansion` role for administrators while still excluding
  settlers; the cleanest split is a per-unit `conquers`/`administrator` flag, since both are `Expansion`).
- **Balance (P7) in a new `conquest.toml`** (+ `infrastructure::balance`): `startingLoyalty` (100),
  `postConquestLoyalty`, `loyaltyRegenPerHour`, `loyaltyDropMin`/`loyaltyDropMax`, and the administrator
  unit id(s) per tribe (or a `conquers` flag on the unit). The capital exception and the slot gate reuse
  013 rules — no new balance.
- **Loyalty visibility.** A player sees **their own** villages' loyalty on the village page (013 panel
  area). An enemy's exact loyalty is revealed via a **scout** (010) or inferred from a **battle report**
  (AC10); the map does not expose it (P4 — only public layout/ownership is public, GDD §7.3).

## Open questions

- **Administrator combat exclusion.** Settlers and scouts are excluded from the main battle; the GDD has
  administrators **fight**. Confirm the per-unit split (a `conquers` flag vs. the shared `Expansion`
  role) and that administrators contribute attack/defence while settlers/scouts do not. **Resolve in the
  plan.**
- **Drop model.** Per-administrator seeded `[min,max]` (proposed, mirrors luck) vs. a fixed per-admin
  value. Faithful Travian is a random ~20–30 per senator; finalise the range in balance.
- **Returning the conquered village's third-party reinforcements vs. wiping them.** Proposed: **return
  home** (007), matching how a losing defender's allies survive a raid. Confirm vs. wipe.
- **Losing your last village.** Proposed: allowed (account becomes village-less; out of scope to
  recover). Confirm no special protection beyond the capital.
