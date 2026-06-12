# Feature 012 — Oases: clear & occupy

**Status:** Draft
**Depends on:** 006 (oasis tiles + bonuses + seeded map), 007 (troop movement engine), 009 (combat resolution + battle reports), 003 (the Outpost is a built building), 002 (production the bonus boosts)
**Roadmap:** M4 · slice 012 · GDD §7.4, §4.4, §7.1 — the PvE/contest layer: clear the wild animals, occupy an oasis through the **Outpost**, hold it for a production bonus, and lose it to a stronger neighbour.

## Goal

The map's **oasis** tiles (006) become a contested resource. Each is guarded by **wild animals**; a
player **clears** them by attacking (009 on the 007 engine) and, if their village's **Outpost** has
free capacity, **occupies** the oasis in the same strike. An occupied oasis adds its **production
bonus** to the owning village (002). Players **reinforce** their oases with stationed troops to hold
them, and an enemy who beats those defenders **takes** the oasis. This is the faithful "clear then
hold" contest (GDD §7.4) — without a hero, gated by the **Outpost**.

## Concepts

- **Oasis tile & wild animals.** An `Oasis` tile (006) carries a per-resource **production bonus** and,
  until cleared, a **wild-animal garrison** — a seeded, deterministic defence (P6) of non-tribe
  "animal" units (lions, crocodiles, …) with defence stats but no offence. Oasis state (owner,
  remaining animals, stationed reinforcements) is **persisted lazily** — the row is materialised the
  first time the oasis is fought or occupied; before that the animals are the seeded default.
- **The Outpost.** A new village building (003). Its **level sets how many oases** that village may
  occupy at once (faithful thresholds, e.g. 1 / 2 / 3 at rising levels — balance). A village with no
  Outpost (level 0) can clear animals but **cannot occupy**.
- **Clear & occupy (one strike).** Attacking an **unoccupied** oasis fights its **wild animals** (009
  battle; no Wall, no morale — animals have no population). If the attacker **prevails** and the
  attacker's village has **free Outpost capacity**, the village **occupies** the oasis in the same
  resolution (owner set, animals cleared). Prevail **without** capacity ⇒ the animals are cleared but
  the oasis stays **unoccupied** (and its animals regrow over time, AC).
- **Reinforce & hold.** The owner (and confederated allies later) may **send troops to an oasis they
  own** (007 reinforcement to a non-village tile); the troops **station** at the oasis and **defend**
  it. An occupied oasis applies its **bonus** to the owner's production (002, compute-on-read) — and
  multiple occupied oases **stack**.
- **Lose.** Attacking an **enemy-occupied** oasis fights the **stationed defenders**; if the attacker
  prevails and has **free capacity**, **ownership transfers** to them; with no capacity the oasis is
  **freed** (owner cleared, animals regrow). The previous owner loses the bonus.
- **Reports.** Every oasis battle emits a **battle report** to the attacker (and the defending owner,
  if any) — forces, losses, who won, and whether the oasis changed hands — on the 009 rails.

## User stories

- As a **player**, I want to **clear and occupy** a nearby oasis, so my village produces more.
- As a **player**, I want my **Outpost** to let me hold several oases as it grows.
- As a **player**, I want to **reinforce** an oasis I own so a raider can't just walk in.
- As a **player**, I want to **take** a rival's poorly-defended oasis.
- As a **player**, I want a **report** of each oasis fight and whether it changed hands.

## Acceptance criteria

> All oasis resolution is server-authoritative (P4) and deterministic given the persisted inputs + the
> world seed (P6/P2): the animal garrison, the battle, occupation, casualties, the bonus, and the
> reports are computed server-side; the client only issues the command.

- **AC1 — Wild animals (seeded).** An un-fought oasis is defended by a **wild-animal garrison** that is
  a pure function of the world seed + the tile coordinate (P6) — same seed + tile ⇒ same animals.
  Animals have defence (vs infantry / vs cavalry) but **no attack**; they never leave the oasis.

- **AC2 — Send an attack at an oasis.** Given a player's village with a garrison, when they send troops
  to an **oasis tile** (another tile holding an oasis), those troops **leave the garrison** and a
  movement is created arriving at `now + travelTime` (007, P7). Rejected with **nothing removed** when
  the target tile is **not an oasis**, the composition is empty/over the garrison, or it is the
  player's **own occupied** oasis (use *reinforce* for that).

- **AC3 — Clear combat.** At arrival the attacker fights the oasis's **current defenders** — the **wild
  animals** if unoccupied, else the **owner's stationed troops** — via the 009 battle: power-law
  casualties on both sides, **no Wall bonus and no morale** (animals/oasis have no population). The
  defenders' losses persist on the oasis; the attacker's survivors return home (009).

- **AC4 — Auto-occupy on a winning clear.** If the attacker **prevails** over an **unoccupied** oasis
  and the attacking village has **free Outpost capacity** (occupied oases `<` Outpost capacity), the
  village **occupies** it: `owner = that village`, animals cleared. If the attacker prevails but has
  **no free capacity**, the animals are cleared but the oasis stays **unoccupied**.

- **AC5 — Take an occupied oasis.** Attacking an **enemy-occupied** oasis fights the **owner's
  stationed defenders**; if the attacker prevails and has **free capacity**, **ownership transfers** to
  the attacker's village (defenders wiped/returned per 009); with **no capacity**, the oasis is
  **freed** (owner cleared). A losing attacker takes casualties and changes nothing.

- **AC6 — Outpost capacity (P4).** A village may occupy **at most** `outpostCapacity(level)` oases
  (balance; level 0 ⇒ 0). Occupation beyond capacity never happens (AC4/AC5 fall back to clear/free).
  The Outpost is built/upgraded like any building (003).

- **AC7 — Reinforce an oasis.** A player may send troops to an oasis **they own**; the troops travel
  (007) and **station** at the oasis as its defenders, and can be **sent back** home (a return). They
  defend it against AC3/AC5. Reinforcing a tile that is not your own oasis is rejected (P4).

- **AC8 — Hold / production bonus (P1).** Each oasis a village occupies adds its per-resource **bonus**
  to that village's production, applied **on read** (002 compute-on-read); multiple occupied oases
  **stack**. Losing/freeing an oasis removes its bonus immediately on the next read.

- **AC9 — Animals regrow.** An **unoccupied, cleared** oasis **regrows** wild animals over time (a
  due-event, P1) back toward the seeded strength, so an un-held oasis becomes contested again.

- **AC10 — Determinism & exactly-once (P2/P6).** The animal garrison, the battle, the occupation
  decision, casualties, the bonus, and the reports are reproducible from persisted state + seed; the
  resolution (casualties + occupation flip + survivor return + report) applies in **one transaction**,
  surviving a restart (orphan-requeue safe).

- **AC11 — Reports.** Every oasis battle persists a **battle report** to the attacker, and to the
  **defending owner** when there is one — forces, losses, who won, and whether the oasis changed hands
  — on the 009 report rails, readable in the existing inbox.

- **AC12 — Interface.** The **map** shows oasis tiles with their bonus and **occupation owner**; the
  player can **attack** or (for their own oasis) **reinforce** an oasis from the **Rally Point**; the
  **Outpost** is buildable; the village page shows its **occupied oases** and the bonus they grant.
  Unavailable actions aren't offered (and are rejected server-side regardless, P4).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/build/view oases (redirected to login). |
| **Player** | Attack any oasis from their **own** village; occupy a cleared oasis up to their Outpost capacity; reinforce/recall **their own** oases; build an Outpost; read **their own** oasis reports; gain the bonus of oases they hold. | Send another player's troops; occupy past capacity; reinforce a tile that is not their own oasis; forge the animals, battle, occupation, casualties, bonus, or report; read others' reports. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel/build times; the seed governs animal generation. | — (superset). |
| **System** | *(system-initiated)* Generate/regrow animals, resolve oasis battles, flip occupation, return survivors, apply the bonus on read, emit reports — exactly once at the due time. | — |

## Out of scope

- **Confederation/alliance defence of oases** (allies reinforcing) → after alliances (015); 012 allows
  **only the owner** to reinforce.
- **Oasis loot** (raiding animals for resources, GDD §7.1) → the reward in 012 is the **bonus**, not
  loot; survivors return empty from an oasis.
- **Catapults/rams at an oasis** (oases have no buildings/Wall) — siege is ignored against an oasis.
- **Scouting an oasis** — not modelled (010 targets villages).
- **The hero / adventures** — removed by design (the Outpost replaces the Hero's Mansion).
- **Oasis-tile field bonuses beyond production %** (oasis-specific crop, etc.) — only the §7.1
  production bonus is applied.

## Decisions

- **Wild animals are a separate non-tribe roster** (`UnitRules.wild_animals: Vec<UnitSpec>`, loaded from
  a `[wild_animals]` balance section), not a fourth `Tribe` — keeping the tribe model (3 player tribes)
  intact. Animals have `defense_infantry`/`defense_cavalry`, `attack = 0`, and are looked up by id for
  the oasis garrison. The **per-oasis garrison composition** is seeded from the world seed + coordinate
  (006 hashing), scaled by distance-from-centre or the tile's bonus (balance) — exact numbers are data.
- **Oasis state is persisted lazily** in a new `oases` table (`world_id, x, y, owner_village NULL,`
  ...) plus an **oasis-garrison** child table (`(world_id, x, y, unit_id, count)`) reused for **both**
  the wild animals (the seeded default, materialised on first fight) and the owner's stationed
  reinforcements. An un-materialised oasis uses the seeded animals computed on read.
- **Combat reuses 009** with the oasis as the defender: a new `BattleInput` path with `wall_level = 0`
  and **morale = 1** (no defender population). The application gathers the oasis defenders (animals or
  stationed troops) instead of a village garrison; `resolve_battle` is unchanged (P3). `order_attack`
  /`process_due_combat` gain an **oasis target** branch (target is a tile, not a village), mirroring how
  the village path resolves — likely a parallel `DueAttack.target_oasis` discriminator or a separate
  `order_attack_oasis`/processor.
- **Occupation** is decided in the resolver: on a winning attack, if the attacker's village has free
  capacity (`occupied_count(village) < outpostCapacity(outpost_level)`), set `owner`; the flip + the
  defender clear + the survivor return + the report are one transaction (exactly-once).
- **The production bonus is a domain hook**: `production_rates` gains an `oasis_bonus: ResourcePercent`
  parameter (the summed % of the village's occupied oases); the application reads the village's occupied
  oases and passes the total. Compute-on-read (P1) — no stored production.
- **The Outpost is a new `BuildingKind::Outpost`** threaded through the balance/repo/web mappings (the
  009-Wall / 011-Cranny set), with cost/time/prereq in `construction.toml`, a population row, and a
  per-level **oasis-capacity** table in balance.
- **Reinforcement to an oasis** extends 007: the movement target is an **oasis tile**; the apply
  stations the troops in the oasis-garrison table (owner side) instead of a village's `reinforcements`;
  the return path is the existing `return` kind. Recall is the existing send-back.
- **Animal regrowth** is a per-oasis **due-event** (P1): an unoccupied oasis schedules a regrow that
  tops its animals back toward the seeded strength; occupying cancels it, freeing reschedules it.

## Open questions

- **Phasing.** This is a large slice. Proposed task phasing: (1) wild-animal roster + seeded oasis
  garrison (domain/balance); (2) `oases` + garrison tables + repo; (3) the Outpost building; (4) oasis
  attack + clear/occupy resolution (no reinforcement yet); (5) the production bonus; (6) oasis
  reinforcement + lose; (7) animal regrowth; (8) web (map occupation, send/reinforce, Outpost, bonus);
  (9/10) docs + review. **Confirm or re-shape in the plan.**
- **Morale at an oasis.** Proposed: **no morale** (animals/oasis have no population) — a big player vs a
  weak oasis is not dampened. Flag if morale should apply when taking an *occupied* (player-owned) oasis.
- **Animal regrowth cap & rate.** Proposed: regrow toward the original seeded strength at a balance
  rate; never exceed it. Finalised in balance.
