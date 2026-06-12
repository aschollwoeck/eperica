# Feature 012 — Oases: clear & occupy — Technical Plan

**Status:** Draft
**Spec:** ./spec.md

Oases become persisted, contestable entities on the existing seeded map. The **battle math is
unchanged** (009 `resolve_battle`) — an oasis is just a defender with **no Wall and no morale**. The
work is: a non-tribe **wild-animal roster**, lazy **oasis state** (owner + garrison) persistence, a new
**Outpost** building gating occupation, an **oasis target** branch in the movement/combat engine
(targeting a tile, not a village), **reinforcement to an oasis**, an **oasis-bonus** hook in
production, and **animal regrowth** as a due-event. No new external deps.

## Constitution check

- **P1 (event-driven):** oasis battles, occupation flips, survivor returns, and **animal regrowth** are
  due-events; the **production bonus** is computed-on-read (no stored production). Nothing polls.
- **P2 (reproducible):** the seeded animal garrison + the battle + the occupation decision fully
  determine the outcome; the casualty/occupy/return/report apply happens in **one transaction**
  (exactly-once, orphan-requeue safe). State is re-derivable from persisted rows + seed.
- **P3 (pure domain):** the wild-animal generation, the battle, and the bonus application are pure over
  numbers/seed; reading the oasis/garrison/buildings is **application** I/O.
- **P4 (server authority):** the client sends only `(target tile, troops[, reinforce])`; the animals,
  battle, occupation, capacity check, casualties, bonus, and reports are server-computed.
- **P6 (seeded determinism):** the per-oasis animal composition is `splitmix64(world_seed, x, y)`
  (the 006/009 hashing); the regrow target is the same seeded strength.
- **P7 (configurable):** wild-animal stats, the per-oasis garrison scaling, the Outpost cost/time/
  **capacity-per-level**, and the regrow rate are balance data.
- **P11 (performance):** the village economy read gains **one** indexed lookup of the village's occupied
  oases (their summed bonus); the oasis battle reuses the indexed claim/apply of 009.

## Domain (`domain`, pure)

- `units.rs` — `UnitRules` gains `wild_animals: Vec<UnitSpec>` + `wild_animal_roster()`; the validator
  leaves them unconstrained (not a tribe roster). Animals carry `defense_infantry`/`defense_cavalry`,
  `attack = 0`, `role` = a new `UnitRole::Wild` (excluded from offence everywhere, like Scout).
- `map.rs`/new `oasis.rs` — `oasis_garrison(world_seed, coord, animals, rules) -> UnitCounts`: a pure,
  seeded composition (count + types scaled by a balance factor, e.g. stronger far from centre). Used as
  the **default defenders** of an un-materialised oasis and as the **regrow target**.
- `combat.rs` — no formula change. A helper `oasis_battle_input(attacker_power, defenders_def) ->
  BattleInput` with `wall_level = 0`, equal populations ⇒ **morale 1** (spec Decision). `add_defense`
  already sums a composition; animals/stationed troops feed it.
- `economy.rs` — `production_rates`/`compute_economy`/`settle_amounts` gain an **`oasis_bonus:
  OasisBonus`** parameter (per-resource %, default zero); applied **after** the base+speed scale:
  `wood = scale(base) + scale(base)·oasis.wood/100` (integer, floor). `OasisBonus` already exists (006).
- `building.rs` — `BuildingKind::Outpost`; a balance `outpost_capacity(level) -> u8` (on the build/
  economy rules) gives the oasis cap.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `units.toml` — a `[wild_animals]` roster (e.g. rat/spider/snake/bat/boar/wolf/bear/croc/tiger/
  elephant with rising defence) loaded into `wild_animals`.
- `map.toml`/`combat.toml` — oasis-garrison **scaling** (base animal strength, growth with distance)
  and **regrow rate**.
- `construction.toml` — `[buildings.outpost]` (cost/time/prereq, e.g. Rally Point + Main Building);
  `economy.toml` — Outpost **population** row; an **`outpost_capacity_per_level`** table (e.g.
  `[0,1,1,2,2,3,…]`). `BuildingKind::Outpost` threaded through every mapping (the 011-Cranny set).

## Persistence (`infrastructure` + migration `0015_oases.sql`)

- `oases(world_id, x, y, owner_village uuid NULL REFERENCES villages, materialised bool NOT NULL,
  PRIMARY KEY (world_id, x, y))` — a row exists once the oasis is fought/occupied; `owner_village` NULL
  ⇒ unoccupied (animals defend), set ⇒ occupied (stationed troops defend). Index on `(owner_village)`.
- `oasis_garrison(world_id, x, y, unit_id, count, PRIMARY KEY (world_id, x, y, unit_id))` — the oasis's
  **current defenders**: the (materialised, possibly regrown) wild animals when unoccupied, or the
  **owner's stationed reinforcements** when occupied.
- `troop_movements` — make `deliver_village` **nullable** (an oasis movement has no destination
  village; `dest_x/dest_y` already identify the tile). New `MovementKind` values `OasisAttack` and
  `OasisReinforce` (the kind CHECK widened); `Return` reused for recalls.
- Repo (`PgAccountRepository`): `oasis_at(coord) -> Option<OasisState>` (owner + materialised),
  `oasis_defenders(coord) -> UnitCounts` (materialised garrison, or seeded animals if absent),
  `occupied_oases(village) -> Vec<(Coordinate, OasisBonus)>` (for the capacity check + the bonus),
  `start_oasis_attack`/`start_oasis_reinforce` (debit garrison + movement with NULL deliver),
  `claim_due_oasis_attacks`, `apply_oasis_battle` (one tx: write defenders' after-counts, flip owner on
  a winning occupy/take/free, schedule survivor return, insert report, mark done),
  `apply_oasis_reinforce` (station troops), `schedule/claim/apply_oasis_regrow`.

## Application (`application`)

- A new `oasis.rs` use-case module: `order_oasis_attack` (validate own village, garrison, target is an
  oasis on another tile, not your own occupied oasis; travel; debit; schedule `OasisAttack`),
  `order_oasis_reinforce` (target is **your** oasis), `process_due_oasis_combat` (gather attacker pools
  + the oasis's defenders + the attacker's Outpost capacity + current occupied count; `resolve_battle`
  with the no-Wall/no-morale input; apply casualties to defenders; decide **occupy / take / free**;
  survivor return; report), `process_due_oasis_regrow`.
- `order_attack` (009) gains an early branch: if the target tile holds **no village but an oasis**,
  route to `order_oasis_attack` (or the web routes it). The combat report reuses 009's
  `NewBattleReport` (the defender "village" fields hold the oasis coord + a synthetic name).
- **Production bonus:** `load_economy` and every settle path (`settle_amounts` callers — build, train,
  starvation, trade, combat loot) pass the village's summed `oasis_bonus` (via a new
  `accounts.village_oasis_bonus(village) -> OasisBonus`). Threaded as task 5; default-zero keeps each
  site compiling before it is wired.
- The scheduler ticks `process_due_oasis_combat` + `process_due_oasis_regrow`; the orphan requeue
  covers the new movement kinds.

## Interface (`web`)

- **Map** — oasis cells show their bonus and, when occupied, the owner; an **Attack** link (and, for
  your own oasis, **Reinforce**) opens the Rally Point pre-filled with the tile.
- **Rally Point** — sending to an oasis tile routes to the oasis attack/reinforce use-case; the catapult/
  scout selectors are hidden for oasis targets.
- **Village page** — lists the village's **occupied oases** and the production bonus they grant; the
  **Outpost** building shows its oasis capacity.
- **Reports** — oasis battles appear in the existing inbox (forces, losses, who won, changed hands).

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: `oasis_garrison` is deterministic from seed+coord; animals have attack 0. |
| AC2 | app (fakes): an oasis attack debits + schedules; rejects non-oasis / over-garrison / own-occupied. |
| AC3 | domain/app: the oasis battle (no Wall, morale 1) applies power-law losses to animals + attacker. |
| AC4 | app/infra: a winning clear with free Outpost capacity occupies; without capacity, clears only. |
| AC5 | app/infra: beating an enemy-occupied oasis's defenders transfers it (with capacity) or frees it. |
| AC6 | domain/app: `outpost_capacity(level)` caps occupation; over-cap never occupies. |
| AC7 | infra (DB): reinforcing an owned oasis stations troops; recall returns them; they defend (AC3). |
| AC8 | domain: `production_rates` applies + stacks oasis bonuses; infra: a village's read includes them. |
| AC9 | infra (DB): an unoccupied cleared oasis regrows animals toward the seeded strength; occupying cancels it. |
| AC10 | infra (DB): resolution applies casualties + occupy + return + report once; crash-resume safe. |
| AC11 | infra/web: an oasis battle report is persisted + shown to both parties. |
| AC12 | web integration: clear+occupy an oasis (PRG), the map shows ownership + the village shows the bonus; Outpost buildable. |

## Notes / open risks

- **`compute_economy` signature churn:** the `oasis_bonus` parameter touches every settle site. Task 5
  threads it with a default-zero first (compiles), then wires the real per-village bonus — mirroring how
  009's `siege_kind`/010's `scouting` field additions were rolled out.
- **Nullable `deliver_village`** is the one schema change with cross-cutting reach (the movement
  apply/claim already branch on kind; the oasis kinds get their own claim/apply, so the reinforce/return
  village path is untouched for `deliver_village IS NOT NULL`).
- **Phasing** (tasks T1–T10) lets each phase land green: animals (T1) and the Outpost (T3) are
  standalone; clear/occupy (T4) works before reinforcement (T6); the bonus (T5) is independent.
