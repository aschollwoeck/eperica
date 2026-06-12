# Feature 013 — Settling & culture points

**Status:** Draft
**Depends on:** 005 (training + the garrison/queue engine; the `Residence` building kind + `Expansion` unit role already scaffolded), 007 (troop movement engine), 006 (the seeded map + free-valley placement), 003 (buildings), 002 (per-village economy the new buildings extend)
**Roadmap:** M5 · slice 013 · GDD §3.3, §3.4, §11.1, §4.2, §6.1 — the multi-village layer: produce **culture points**, train **settlers**, and found new villages on free tiles; designate an unconquerable **capital** via the **Palace**.

## Goal

A player starts with **one** village and grows by **settling**. This slice makes a player **multi-village**: villages produce **culture points (CP)** over time (compute-on-read, P1); CP + a **Residence/Palace** unlock **expansion slots**; the player trains **settlers** and sends them to a **free valley** to found a new village, which runs its own independent economy/queues/defence. Building a **Palace** designates the player's **capital** — the one village that may raise its resource fields **beyond the normal cap** and (from 014) **cannot be conquered**. Conquest itself is out of scope (014); this slice delivers the **peaceful** expansion path and the capital rule.

## Concepts

- **Culture points (CP).** A world-wide, per-**player** accumulator produced **over time** by the
  player's buildings — chiefly the **Town Hall**, with a small base from every village (balance). CP is
  **lazy** (P1): stored as `value + lastUpdated + rate`, computed on read; there is no global tick. CP
  is **never spent** — it is a *threshold* gate: holding the **Nth** village requires at least
  `cpThreshold(n)` cumulative CP (a rising balance table). CP is **per player**, pooled across villages
  (unlike resources, §3.3).
- **Expansion slots.** How many villages a player may hold at once is the **minimum** of (a) what their
  **CP** allows — the largest `n` with `cpThreshold(n) ≤ cp` — and (b) the **expansion capacity** their
  **Residence/Palace** buildings grant (a per-level balance table; a Residence/Palace at level `L` grants
  `expansionSlots(L)` slots). A player may found a new village only with a **free slot** (current village
  count `<` allowed).
- **The Residence and the Palace.** Two new center buildings (§4.2). Both **train settlers** and grant
  **expansion slots**; the **Palace** additionally **designates the capital** (only one Palace per
  player at a time) and is the prerequisite for administrators (conquest, 014). A village holds **at most
  one** of {Residence, Palace}. (Loyalty defence is 014.)
- **The Town Hall.** A new center building (§4.2) that **produces culture points** (its level sets the
  CP/hour it adds; balance). *(Celebrations are out of scope — see below.)*
- **Settlers.** The tribe's **Expansion**-role unit (already in the roster, trained in the
  Residence/Palace, 005 scaffolding gated off until now). Founding a new village consumes a **group of
  settlers** (balance, e.g. 3) sent from a village to a **target tile**.
- **Settling (founding a village).** A player sends `settlersPerVillage` settlers from one of their
  villages to a **free valley tile** (006). On arrival, if the tile is **still free**, the player has a
  **free expansion slot**, and the composition is exactly the settler group, a **new village is
  founded** there (the 006 starting template + starting resources) owned by the player; the settlers are
  consumed. Otherwise the settlers **return home** (the tile was taken, or the slot was lost in flight).
- **The capital.** Building a **Palace** marks that village the player's **capital**. The capital may
  raise its **resource fields beyond the normal level cap** (`capitalFieldMaxLevel > fieldMaxLevel`,
  balance); a non-capital field stops at the normal cap. Exactly **one** capital exists per player;
  building a Palace in another village **relocates** the capital (the old Palace cannot remain — at most
  one Palace per player). The capital flag also records "**unconquerable**" for 014 (no behaviour here
  beyond the flag + the field cap).

## User stories

- As a **player**, I want my villages to **accumulate culture points** so I can expand.
- As a **player**, I want to **train settlers** and **found a new village** on a free tile.
- As a **player**, I want each of my villages to run its **own** economy, build queue, and defence.
- As a **player**, I want to build a **Palace** to set my **capital** and push its fields past the cap.
- As a **player**, I want to be **told** when I can't expand yet (not enough CP, no free slot, no settlers).

## Acceptance criteria

> All expansion is server-authoritative (P4) and reproducible from persisted state (P2): CP, the slot
> gate, settler dispatch, the founding (or bounce), and the capital flag are computed server-side; the
> client only issues commands.

- **AC1 — Culture points accrue (P1).** A player's CP is **computed on read** from stored
  `(value, lastUpdated, rate)` (002 economy model) — never polled. The **rate** is the sum over the
  player's villages of a per-village base plus the village's **Town Hall** contribution (balance);
  raising/founding/losing a Town Hall changes the rate, re-anchored at that instant so no CP is lost or
  double-counted. CP is per **player**, pooled across villages.

- **AC2 — The Town Hall produces CP.** The **Town Hall** is a new buildable center building (003): cost,
  time, prerequisites, population, and a **CP-per-level** table are balance. A village with a higher
  Town Hall raises the player's CP rate; level 0 (none) contributes only the per-village base.

- **AC3 — Residence/Palace are buildable and grant slots.** The **Residence** and **Palace** are new
  buildable buildings (003) with cost/time/prereqs/population (balance). Each grants
  `expansionSlots(level)` **expansion slots**; a village holds **at most one** of the two. Training
  settlers requires a Residence or Palace (005 gating, now enabled).

- **AC4 — Expansion is slot-gated (P4).** The number of villages a player may hold is
  `min(cpAllows(cp), Σ expansionSlots over the player's Residence/Palace)` (the **+1** for the starting
  village is in the balance tables). Founding is **rejected with nothing consumed** when the player has
  **no free slot** (current count ≥ allowed), regardless of resources.

- **AC5 — Train settlers.** A player with a **Residence/Palace** may train their tribe's **settler**
  (Expansion role) like any troop batch (005): cost + time, debited and queued, delivered to the
  village garrison. Settlers carry no attack and provide no defence (Expansion role excluded from
  combat, like Scout).

- **AC6 — Found a new village (settle).** A player sends exactly `settlersPerVillage` settlers from a
  village to a **target valley tile** (007 movement, paced by the settler speed, P7). On arrival, **if**
  the tile is still a **free valley** **and** the player still has a **free expansion slot**, a **new
  village is founded** there — owned by the player, built from the 006 starting template with starting
  resources, its CP rate folded into the player's — and the settlers are **consumed**. The founding is
  one transaction (exactly-once, P2).

- **AC7 — Failed settle bounces home.** If, at arrival, the tile is **no longer a free valley** (taken,
  or not a valley) **or** the player has **no free slot**, the settlers are **not** consumed — they
  travel **back home** (a return) and rejoin the garrison. Nothing is founded.

- **AC8 — A new village is independent.** A founded village has its **own** stored resources, build/unit
  queues, garrison, and economy (002–009 per-village engines), keyed by its own id and tile. There is
  **no** global resource pool; the player now owns multiple villages, each addressable.

- **AC9 — The Palace sets the capital.** Building a **Palace** designates that village the player's
  **capital** and clears any previous Palace/capital (**at most one Palace per player**; building one
  elsewhere **relocates** the capital). Exactly one capital exists per player at any time. A player's
  **first** village is **not** capital until they build a Palace.

- **AC10 — The capital uncaps fields.** A **capital** village may raise its **resource fields** to
  `capitalFieldMaxLevel` (> the normal `fieldMaxLevel`); a **non-capital** village's fields stop at the
  normal cap (003 build validation). Center buildings are unaffected. (The capital's
  un-conquerability is persisted for 014; no conquest behaviour here.)

- **AC11 — Multi-village interface.** A player with more than one village can **see and switch between**
  their villages; each village page shows **that** village's economy/queues. A **village switcher** (or
  list) exposes every owned village; the **map** marks the player's villages (006 already), and the
  **capital** is distinguished. Culture points + expansion slots (used/allowed) are shown, and the
  **settle** action is available from the Rally Point (send settlers to a tile) when a slot is free.

- **AC12 — Determinism & exactly-once (P2/P6).** CP, the slot gate, the founding decision, and the
  capital flag are reproducible from persisted state. Settler dispatch debits once; the founding (or
  bounce) applies in one transaction, orphan-requeue safe. A new village's tile/economy is derived from
  the world seed + template (006), so the same history yields the same world.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Settle / build / view villages (redirected to login). |
| **Player** | Build a Town Hall / Residence / Palace in **their own** villages; train settlers there; **found** a new village on a free valley when they have a free slot + the settlers; designate their capital via a Palace; raise their **capital's** fields past the normal cap; read **their own** CP + slots. | Found past their allowed slot count; settle another player's tile or onto an occupied/non-valley tile; train settlers without a Residence/Palace; hold two Palaces; raise a non-capital's fields past the cap; forge CP, the slot gate, or the founding; act on others' villages except via game mechanics. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales build/train/travel times and the CP rate; map size bounds free tiles. | — (superset). |
| **System** | *(system-initiated)* Accrue CP on read; resolve a due settle into a **founded village** (or a bounce) exactly once; fold a new/lost village's CP into the player's rate — at the due time, from persisted state. | — |

## Out of scope

- **Conquest / administrators / loyalty** (Senator/Chief, ownership transfer) → slice **014**. This
  slice persists the capital's **unconquerable** flag but adds no loyalty/conquest mechanics.
- **Celebrations** (Town Hall one-off CP parties) → later; 013 produces CP **only** from building
  levels over time.
- **Resource-boost buildings** (Sawmill/Brickyard/Iron Foundry/Grain Mill/Bakery, §4.3) → later; the
  capital's benefit here is the **field cap**, not extra multipliers.
- **Palace demolition / capital removal** beyond **relocation** by building a new Palace — a player
  always has exactly one capital once they build their first Palace.
- **Per-village build/train UIs beyond a switcher** — the existing per-village pages are reused; this
  slice only makes them address **each** owned village.

## Decisions

- **CP is a per-player lazy accumulator** mirroring the 002 economy model: a `player_culture(player_id,
  value, rate_per_hour, updated_at)` row, settled-on-read like resources, re-anchored whenever the rate
  changes (a Town Hall built/upgraded, a village founded/lost). The **rate** = `Σ_villages (base +
  townHallCp(level))`, all balance. **No CP is stored per village** — only the player-level accumulator
  + the derivable rate. CP is **never debited**; it only gates.
- **The slot gate is `min(cp-allowed, building-allowed)`.** `cpAllowed(cp)` = largest `n` with
  `cpThreshold(n) ≤ cp` (a rising table, `cpThreshold(1)=0` so the first village is free);
  `buildingAllowed` = `Σ expansionSlots(level)` over the player's Residences/Palaces. A founding needs
  `villageCount < min(...)`. The check is **re-evaluated at arrival** (P4) — the slot could have been
  consumed by another in-flight settle.
- **Settling extends the 007 movement engine** with a `settle` kind: the movement targets a **tile**
  (no destination village — reuse the nullable `deliver_village` from 012), carries the settler group,
  and on arrival the apply **founds a village** (insert the 006 template + starting resources, owner =
  the sender) **or** schedules a **return** (the 012 oasis-bounce pattern). Founding is one transaction;
  the new village's CP folds into the player rate in the same tx (re-anchored).
- **The capital is a flag + a field-cap branch.** A `villages.is_capital` column (or a per-player
  `capital_village`); building a Palace sets it and clears the previous. Construction validation uses
  `capitalFieldMaxLevel` for a capital's resource fields and `fieldMaxLevel` otherwise — a small change
  to the 003 max-level check (the field `LevelSpec` already has a max; the capital raises only fields).
- **Town Hall / Residence / Palace are new `BuildingKind`s** threaded through the balance/repo/web
  mappings (the 011-Cranny / 012-Outpost set), each with cost/time/prereq/population. Settlers reuse the
  existing `UnitRole::Expansion` + `trained_in = Residence`; the 005 `can_train` Residence gate is
  enabled (Palace counts as a Residence for training).
- **Expansion units are non-combatants** (Expansion role excluded from `attack_power`/`add_defense`,
  like Scout/Wild) and travel at their own (slow) speed (007 pacing), so a settle is slow and
  vulnerable in transit.

## Open questions

- **Phasing.** Large slice. Proposed: (1) CP model + Town Hall (domain/balance + per-player accumulator);
  (2) Residence/Palace + expansion-slot rule + settler training enablement; (3) the capital flag +
  field-cap; (4) the `settle` movement (dispatch + found/bounce, persistence); (5) the slot gate at
  dispatch + arrival; (6) web (village switcher, CP/slots panel, settle from Rally Point, capital
  marker); (7) scheduler tick + docs; (8) review. **Confirm/re-shape in the plan.**
- **First-village CP base.** Proposed: every village contributes a small base CP/hour so a new player
  always trickles toward their 2nd village even without a Town Hall; the Town Hall accelerates it.
  Finalised in balance.
- **Capital field cap value.** Proposed `capitalFieldMaxLevel` a few levels above the normal cap;
  exact value balance.
