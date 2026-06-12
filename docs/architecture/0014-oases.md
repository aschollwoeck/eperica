# Oases — clear, occupy, hold, reinforce, lose, regrow

**Status:** Current
**Date:** 2026-06-12 · **Slice:** 012

## Context
The map's **oasis** tiles (006) become a contested PvE/PvP resource (GDD §7.4). Each is guarded by
**wild animals**; a player **clears** them by attacking on the 009 engine and, if their village's
**Outpost** has free capacity, **occupies** the oasis in the same strike. A held oasis adds its
**production bonus** to the owning village (002). Players **reinforce** their oases with stationed
troops to hold them, and an enemy who beats those defenders **takes** the oasis. A cleared, unheld
oasis **regrows** its animals over time, so the contest renews.

## Design
- **The math is the unchanged 009 battle.** An oasis is a defender with **no Wall** and **morale 1**
  (animals/oases have no population): the resolver builds a `BattleInput` with `wall_level = 0` and
  equal populations. `resolve_battle` is reused verbatim (P3). The only new pure code is
  `oasis.rs::oasis_garrison(seed, coord, animals, rules)` — the **seeded** wild-animal garrison (P6:
  a pure function of the world seed + tile, so an un-fought oasis needs no stored state; strength and
  animal tier rise with distance) — and `regrow_step(current, seeded, per_step)`, one top-up tick
  toward the seeded strength. Wild animals are a separate **non-tribe roster** (`UnitRules.wild_animals`,
  `UnitRole::Wild`, attack 0) loaded from `units.toml [[wild_animals]]`; the seeded-garrison and regrow
  constants live in `[oasis_garrison]` (P7).
- **Lazy, world-global state.** An `oases(world_id, x, y, owner_village NULL, materialised, regrow_at)`
  row appears the first time the oasis is fought/occupied; `owner_village NULL` ⇒ unoccupied (animals
  defend, re-derived from the seed), set ⇒ occupied (the owner's stationed troops defend). The
  `oasis_garrison(world_id, x, y, unit_id, count)` child holds the **current** defenders — materialised
  animals while unoccupied, the owner's reinforcements while occupied. An oasis movement targets a
  **tile**, not a village, so `troop_movements.deliver_village` became nullable and gained the
  `oasis_attack` / `oasis_reinforce` kinds (Return is reused for survivor/recall trips).
- **Application orchestrates over persisted state.** `order_oasis_attack` validates the target is an
  oasis the player does not already occupy, debits, and schedules. `process_due_oasis_combat` gathers
  the attacker's pools + the oasis's defenders (animals, or the owner's stationed troops on the owner's
  tribe roster) + the attacker's **Outpost capacity**, resolves, and applies casualties + the
  ownership outcome + a survivor return + the report in **one transaction** (`apply_oasis_battle`,
  exactly-once, orphan-requeue safe via the shared `troop_movements` requeue). Occupation is decided
  against `economy_rules.outpost_capacity(level)`: a winning attack **occupies** with free capacity,
  else **clears** an unoccupied oasis (Unchanged) or **frees** a held one (Free); `AttackMode::Attack`
  wipes the loser, so a taken oasis starts empty.
- **The production bonus rides the village read.** `Village` gained an `oasis_bonus` field the
  repository fills from the village's occupied oases (one indexed lookup, P11). `production_rates` /
  `compute_economy` take it and boost each resource's **gross** field output (floor) before
  population/upkeep subtract from crop — so a +crop oasis correctly lifts net crop. Because
  `settle_amounts` already takes the `&Village`, the bonus threads through every settle/credit site
  with no per-site repository call; compute-on-read keeps it consistent with the display (P1, AC8).
- **Reinforce, recall, regrow.** `order_oasis_reinforce` (your oasis only) debits + schedules an
  `oasis_reinforce` movement; on arrival `apply_oasis_reinforce` **stations** the troops (re-checking
  ownership — a lost-in-flight oasis **bounces** the troops home). `order_oasis_recall` reads+deletes
  the oasis garrison and returns the troops home (the oasis stays owned, undefended). Regrowth is a
  per-oasis due-event on `oases.regrow_at` (set when a battle leaves the oasis unoccupied, NULL when
  occupied): `process_due_oasis_regrow` tops the animals up one `regrow_step` and reschedules until
  full, the apply **guarded** on the still-unoccupied row holding the claimed `regrow_at` — so
  occupying in flight cancels the regrow and a crash re-runs it (P2). The scheduler ticks all three
  oasis processors each loop; none change a village garrison at resolution, so none re-sync starvation.

## Consequences
- Confederation defence of oases (allies reinforcing) waits for alliances (015); 012 lets **only the
  owner** reinforce. Oasis loot (raiding animals for resources) is out of scope — the reward is the
  bonus. Catapults/rams and scouting are ignored against an oasis (no buildings/Wall; 010 targets
  villages).
- Reports reuse the 009 `battle_reports` rails: for a village-less (animal) defender the table's
  `defender_player`/`defender_village` are nullable and the oasis tile + a synthetic label stand in,
  so an oasis fight is readable in the existing inbox (AC11). An occupied-oasis report records the
  owner as the defender, fitting the existing columns.
- The whole loop — garrison, battle, occupation, bonus, regrow — is reproducible from persisted rows +
  the world seed (P2/P6).

## Links
specs/constitution.md (P1–P4, P6, P7, P11); specs/features/012-oases/;
specs/balance/units.toml ([[wild_animals]], [oasis_garrison]), construction.toml ([buildings.outpost]),
economy.toml (outpost population + capacity_per_level); crates/domain/src/oasis.rs (oasis_garrison,
regrow_step, OasisRules), units.rs (UnitRole::Wild, wild_animals), economy.rs (oasis_bonus in
production_rates/compute_economy), village.rs (Village.oasis_bonus), building.rs (Outpost), map.rs
(WorldMap::seed/oasis_bonus_at); crates/application/src/oasis.rs (order/process oasis attack, reinforce,
recall, regrow), ports.rs (OasisRepository + types); crates/infrastructure/src/repo.rs (OasisRepository
impl), event_store.rs (scheduler ticks); crates/web/src/handlers.rs (map oasis links, rally routing,
village oases panel, recall); migrations/0015_oases.sql, 0016_oasis_reports.sql, 0017_oasis_regrow.sql.
